//! Entity management tools - CRUD with command-based operations.

use rmcp::{
    handler::server::wrapper::Parameters,
    model::CallToolResult,
    schemars::{self, JsonSchema},
    tool, tool_router, ErrorData as McpError,
};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::mcp::protocol::Response;
use crate::mcp::server::McpServer;
use crate::models::Entity;
use crate::repositories::EntityRepository;
use crate::services::{
    CreateEntityInput, EntityCommand, EntityService, NewReference, UpdateEntityInput,
};

// ============================================================================
// Parameter Types
// ============================================================================

/// Parameters for create_entity tool.
///
/// Creates a new entity with mandatory classification, optional parents, and commands.
/// At least one Add command is required to attach an initial reference.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateEntityParams {
    /// Human-readable name for the entity.
    pub name: String,
    /// Detailed description (auto-embedded for semantic search).
    pub description: String,
    /// Category IDs for classification (required, non-empty).
    pub category_ids: Vec<String>,
    /// Parent entity IDs for BELONGS_TO relationships.
    /// Required for non-Domain scope entities.
    #[serde(default)]
    pub parent_ids: Vec<String>,
    /// Commands to execute. Must include at least one Add command.
    #[serde(default)]
    pub commands: Vec<EntityCommandInput>,
}

/// Parameters for update_entity tool.
///
/// Updates an existing entity. All fields are optional except entity_id.
/// Categories and parents use replace semantics when provided.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateEntityParams {
    /// Entity ID to update.
    pub entity_id: String,
    /// New name (optional).
    #[serde(default)]
    pub name: Option<String>,
    /// New description (optional, re-embeds if changed).
    #[serde(default)]
    pub description: Option<String>,
    /// Replace category IDs (optional). Replaces all existing categories.
    #[serde(default)]
    pub category_ids: Option<Vec<String>>,
    /// Replace parent IDs (optional). Replaces all existing parents.
    #[serde(default)]
    pub parent_ids: Option<Vec<String>>,
    /// Commands to execute.
    #[serde(default)]
    pub commands: Vec<EntityCommandInput>,
}

/// Command input for entity operations.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EntityCommandInput {
    /// Attach an existing reference to this entity.
    Attach {
        /// ID of the reference to attach.
        reference_id: String,
    },
    /// Detach a reference from this entity.
    Unattach {
        /// ID of the reference to detach.
        reference_id: String,
    },
    /// Create and attach a new reference.
    Add(NewReferenceInput),
    /// Create a RELATED_TO relationship.
    Relate {
        /// Target entity ID.
        entity_id: String,
        /// Optional note (embedded for semantic search).
        #[serde(default)]
        note: Option<String>,
    },
    /// Remove a RELATED_TO relationship.
    Unrelate {
        /// Target entity ID.
        entity_id: String,
    },
    /// Create a code link (Component/Unit only).
    Link {
        /// Target entity ID.
        entity_id: String,
        /// Link type: calls, imports, implements, instantiates.
        link_type: String,
    },
    /// Remove a code link.
    Unlink {
        /// Target entity ID.
        entity_id: String,
        /// Link type: calls, imports, implements, instantiates.
        link_type: String,
    },
}

/// New reference input for Add command.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(tag = "ref_type", rename_all = "snake_case")]
pub enum NewReferenceInput {
    /// Code reference with LSP metadata.
    Code {
        /// Source ID from project config (defaults to "default" if not specified).
        #[serde(default = "crate::config::default_source_id")]
        source_id: String,
        /// Path to the source file.
        document_path: String,
        /// LSP symbol name (e.g., "impl Foo::bar").
        lsp_symbol: String,
        /// Description of what this reference points to.
        description: String,
        /// Start line (optional, from LSP).
        #[serde(default)]
        start_line: Option<u32>,
        /// End line (optional, from LSP).
        #[serde(default)]
        end_line: Option<u32>,
    },
    /// Text reference with line range.
    Text {
        /// Source ID from project config (defaults to "default" if not specified).
        #[serde(default = "crate::config::default_source_id")]
        source_id: String,
        /// Path to the document.
        document_path: String,
        /// Description of what this reference points to.
        description: String,
        /// Starting line number.
        start_line: u32,
        /// Ending line number.
        end_line: u32,
        /// Optional anchor (e.g., "## Section").
        #[serde(default)]
        anchor: Option<String>,
    },
}

/// Parameters for delete_entity tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteEntityParams {
    /// Entity ID to delete.
    pub entity_id: String,
}

// ============================================================================
// Response Types
// ============================================================================

/// Entity result for tool responses.
#[derive(Debug, Serialize)]
pub struct EntityResult {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub parents: Vec<String>,
    pub has_embedding: bool,
}

impl From<Entity> for EntityResult {
    fn from(e: Entity) -> Self {
        Self {
            id: e.id,
            name: e.name,
            description: e.description,
            scope: None,
            categories: Vec::new(),
            parents: Vec::new(),
            has_embedding: e.embedding.is_some(),
        }
    }
}

impl From<crate::services::EntityInfo> for EntityResult {
    fn from(e: crate::services::EntityInfo) -> Self {
        Self {
            id: e.id,
            name: e.name,
            description: e.description,
            scope: Some(e.scope),
            categories: e.categories,
            parents: e.parents,
            has_embedding: true,
        }
    }
}

// ============================================================================
// Input Conversions
// ============================================================================

impl From<EntityCommandInput> for EntityCommand {
    fn from(input: EntityCommandInput) -> Self {
        match input {
            EntityCommandInput::Attach { reference_id } => EntityCommand::Attach { reference_id },
            EntityCommandInput::Unattach { reference_id } => {
                EntityCommand::Unattach { reference_id }
            }
            EntityCommandInput::Add(r) => EntityCommand::Add(r.into()),
            EntityCommandInput::Relate { entity_id, note } => {
                EntityCommand::Relate { entity_id, note }
            }
            EntityCommandInput::Unrelate { entity_id } => EntityCommand::Unrelate { entity_id },
            EntityCommandInput::Link {
                entity_id,
                link_type,
            } => EntityCommand::Link {
                entity_id,
                link_type: parse_link_type(&link_type),
            },
            EntityCommandInput::Unlink {
                entity_id,
                link_type,
            } => EntityCommand::Unlink {
                entity_id,
                link_type: parse_link_type(&link_type),
            },
        }
    }
}

impl From<NewReferenceInput> for NewReference {
    fn from(input: NewReferenceInput) -> Self {
        match input {
            NewReferenceInput::Code {
                source_id,
                document_path,
                lsp_symbol,
                description,
                start_line,
                end_line,
            } => NewReference::Code {
                source_id,
                document_path,
                lsp_symbol,
                description,
                start_line,
                end_line,
            },
            NewReferenceInput::Text {
                source_id,
                document_path,
                description,
                start_line,
                end_line,
                anchor,
            } => NewReference::Text {
                source_id,
                document_path,
                description,
                start_line,
                end_line,
                anchor,
            },
        }
    }
}

fn parse_link_type(s: &str) -> crate::services::LinkType {
    match s.to_lowercase().as_str() {
        "calls" => crate::services::LinkType::Calls,
        "imports" => crate::services::LinkType::Imports,
        "implements" => crate::services::LinkType::Implements,
        "instantiates" => crate::services::LinkType::Instantiates,
        _ => crate::services::LinkType::Calls, // Default
    }
}

/// Response for create_entity tool.
#[derive(Debug, Serialize)]
pub struct CreateEntityResult {
    pub entity: EntityResult,
    pub executed: Vec<crate::services::ExecutedCommand>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed: Option<crate::services::FailedCommand>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<crate::services::EntityCommand>,
}

/// Response for update_entity tool.
#[derive(Debug, Serialize)]
pub struct UpdateEntityResult {
    pub entity: EntityResult,
    pub embedding_updated: bool,
    pub executed: Vec<crate::services::ExecutedCommand>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed: Option<crate::services::FailedCommand>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<crate::services::EntityCommand>,
}

/// Response for delete_entity tool.
#[derive(Debug, Serialize)]
pub struct DeleteEntityResult {
    pub deleted: bool,
    pub entity_id: String,
}

// ============================================================================
// Tool Router
// ============================================================================

#[tool_router(router = entity_tools, vis = "pub(crate)")]
impl McpServer {
    /// Create a new entity in the knowledge graph.
    ///
    /// Requires category_ids (non-empty) and at least one Add command.
    /// Non-Domain scope entities also require parent_ids.
    #[tool(description = "Create a new entity with auto-embedding of description.")]
    pub async fn create_entity(
        &self,
        Parameters(params): Parameters<CreateEntityParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::info!(name = %params.name, "Running create_entity tool");

        let entity_service = self.resolve::<EntityService>();

        let input = CreateEntityInput {
            name: params.name,
            description: params.description,
            category_ids: params.category_ids,
            parent_ids: params.parent_ids,
            commands: params.commands.into_iter().map(|c| c.into()).collect(),
        };

        let output = entity_service
            .create(input)
            .await
            .map_err(|e: AppError| McpError::from(e))?;

        let response = CreateEntityResult {
            entity: output.entity.into(),
            executed: output.executed,
            failed: output.failed,
            skipped: output.skipped,
        };

        tracing::info!(id = %response.entity.id, "Created entity");

        Response(response, None).into()
    }

    /// Update an entity's name, description, categories, parents, or execute commands.
    ///
    /// If description changes, the embedding is regenerated.
    /// Categories and parents use replace semantics when provided.
    #[tool(description = "Update an entity. Re-embeds if description changes.")]
    pub async fn update_entity(
        &self,
        Parameters(params): Parameters<UpdateEntityParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::info!(id = %params.entity_id, "Running update_entity tool");

        let entity_service = self.resolve::<EntityService>();

        let input = UpdateEntityInput {
            entity_id: params.entity_id,
            name: params.name,
            description: params.description,
            category_ids: params.category_ids,
            parent_ids: params.parent_ids,
            commands: params.commands.into_iter().map(|c| c.into()).collect(),
        };

        let output = entity_service
            .update(input)
            .await
            .map_err(|e: AppError| McpError::from(e))?;

        let response = UpdateEntityResult {
            entity: output.entity.into(),
            embedding_updated: output.embedding_updated,
            executed: output.executed,
            failed: output.failed,
            skipped: output.skipped,
        };

        tracing::info!(
            id = %response.entity.id,
            embedding_updated = response.embedding_updated,
            "Updated entity"
        );

        Response(response, None).into()
    }

    /// Delete an entity from the knowledge graph.
    ///
    /// Fails if the entity has children. Document references are cascaded.
    #[tool(description = "Delete an entity (must have no children).")]
    pub async fn delete_entity(
        &self,
        Parameters(params): Parameters<DeleteEntityParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::info!(id = %params.entity_id, "Running delete_entity tool");

        let entity_repo = self.resolve::<EntityRepository>();

        // Check for children first
        let children = entity_repo
            .get_children(&params.entity_id)
            .await
            .map_err(|e: AppError| McpError::from(e))?;

        if !children.is_empty() {
            return Err(McpError::invalid_params(
                format!(
                    "Entity '{}' has {} children and cannot be deleted",
                    params.entity_id,
                    children.len()
                ),
                None,
            ));
        }

        entity_repo
            .delete(&params.entity_id)
            .await
            .map_err(|e: AppError| McpError::from(e))?;

        let response = DeleteEntityResult {
            deleted: true,
            entity_id: params.entity_id.clone(),
        };

        tracing::info!(id = %params.entity_id, "Deleted entity");

        Response(response, None).into()
    }
}
