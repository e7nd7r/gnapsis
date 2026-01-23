//! Entity management tools - CRUD, classification, and relationships.

use rmcp::{
    handler::server::wrapper::Parameters,
    model::CallToolResult,
    schemars::{self, JsonSchema},
    tool, tool_router, ErrorData as McpError,
};
use serde::{Deserialize, Serialize};

use crate::context::AppEmbedder;
use crate::error::AppError;
use crate::mcp::protocol::Response;
use crate::mcp::server::McpServer;
use crate::models::Entity;
use crate::repositories::{CategoryRepository, DocumentRepository, EntityRepository};
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

/// Parameters for classify tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ClassifyParams {
    /// Entity ID to classify.
    pub entity_id: String,
    /// Category IDs to apply.
    pub category_ids: Vec<String>,
}

/// Parameters for unclassify tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct UnclassifyParams {
    /// Entity ID to unclassify.
    pub entity_id: String,
    /// Category IDs to remove.
    pub category_ids: Vec<String>,
}

/// Parameters for add_belongs tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct AddBelongsParams {
    /// Child entity ID.
    pub child_id: String,
    /// Parent entity IDs.
    pub parent_ids: Vec<String>,
    /// Optional note for the relationship.
    #[serde(default)]
    pub note: Option<String>,
}

/// Parameters for add_related tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct AddRelatedParams {
    /// Source entity ID.
    pub from_id: String,
    /// Target entity IDs.
    pub to_ids: Vec<String>,
    /// Optional relation type description.
    #[serde(default)]
    pub relation_type: Option<String>,
    /// Optional note describing the relationship (auto-embedded for semantic search).
    #[serde(default)]
    pub note: Option<String>,
}

/// Parameters for add_link tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct AddLinkParams {
    /// Source entity ID.
    pub from_id: String,
    /// Target entity ID.
    pub to_id: String,
    /// Link type: CALLS, IMPORTS, IMPLEMENTS, or INSTANTIATES.
    pub link_type: String,
}

/// Parameters for remove_references tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RemoveReferencesParams {
    /// Reference IDs to remove.
    pub reference_ids: Vec<String>,
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
                document_path,
                lsp_symbol,
                description,
                start_line,
                end_line,
            } => NewReference::Code {
                document_path,
                lsp_symbol,
                description,
                start_line,
                end_line,
            },
            NewReferenceInput::Text {
                document_path,
                description,
                start_line,
                end_line,
                anchor,
            } => NewReference::Text {
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

/// Response for classify/unclassify tools.
#[derive(Debug, Serialize)]
pub struct ClassifyResult {
    pub entity_id: String,
    pub category_ids: Vec<String>,
    pub action: String,
}

/// Response for add_belongs tool.
#[derive(Debug, Serialize)]
pub struct AddBelongsResult {
    pub child_id: String,
    pub parent_ids: Vec<String>,
}

/// Response for add_related tool.
#[derive(Debug, Serialize)]
pub struct AddRelatedResult {
    pub from_id: String,
    pub to_ids: Vec<String>,
}

/// Response for add_link tool.
#[derive(Debug, Serialize)]
pub struct AddLinkResult {
    pub from_id: String,
    pub to_id: String,
    pub link_type: String,
}

/// Response for remove_references tool.
#[derive(Debug, Serialize)]
pub struct RemoveReferencesResult {
    pub removed_count: usize,
    pub reference_ids: Vec<String>,
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

        Response(response).into()
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

        Response(response).into()
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

        Response(response).into()
    }

    /// Classify an entity with one or more categories.
    #[tool(description = "Classify an entity with categories.")]
    pub async fn classify(
        &self,
        Parameters(params): Parameters<ClassifyParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::info!(
            entity_id = %params.entity_id,
            categories = ?params.category_ids,
            "Running classify tool"
        );

        let entity_repo = self.resolve::<EntityRepository>();
        let category_repo = self.resolve::<CategoryRepository>();

        // Verify entity exists
        entity_repo
            .find_by_id(&params.entity_id)
            .await
            .map_err(|e: AppError| McpError::from(e))?
            .ok_or_else(|| {
                McpError::invalid_params(format!("Entity not found: {}", params.entity_id), None)
            })?;

        // Verify categories exist and classify
        for cat_id in &params.category_ids {
            category_repo
                .find_by_id(cat_id)
                .await
                .map_err(|e: AppError| McpError::from(e))?
                .ok_or_else(|| {
                    McpError::invalid_params(format!("Category not found: {}", cat_id), None)
                })?;

            entity_repo
                .classify(&params.entity_id, cat_id)
                .await
                .map_err(|e: AppError| McpError::from(e))?;
        }

        let response = ClassifyResult {
            entity_id: params.entity_id,
            category_ids: params.category_ids,
            action: "classified".to_string(),
        };

        tracing::info!(
            entity_id = %response.entity_id,
            count = response.category_ids.len(),
            "Classified entity"
        );

        Response(response).into()
    }

    /// Remove classifications from an entity.
    #[tool(description = "Remove category classifications from an entity.")]
    pub async fn unclassify(
        &self,
        Parameters(params): Parameters<UnclassifyParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::info!(
            entity_id = %params.entity_id,
            categories = ?params.category_ids,
            "Running unclassify tool"
        );

        let entity_repo = self.resolve::<EntityRepository>();

        for cat_id in &params.category_ids {
            entity_repo
                .unclassify(&params.entity_id, cat_id)
                .await
                .map_err(|e: AppError| McpError::from(e))?;
        }

        let response = ClassifyResult {
            entity_id: params.entity_id,
            category_ids: params.category_ids,
            action: "unclassified".to_string(),
        };

        tracing::info!(
            entity_id = %response.entity_id,
            count = response.category_ids.len(),
            "Unclassified entity"
        );

        Response(response).into()
    }

    /// Add composition relationships (child BELONGS_TO parents).
    ///
    /// Scope validation is enforced by Neo4j trigger.
    #[tool(description = "Add composition: child BELONGS_TO parent entities.")]
    pub async fn add_belongs(
        &self,
        Parameters(params): Parameters<AddBelongsParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::info!(
            child = %params.child_id,
            parents = ?params.parent_ids,
            "Running add_belongs tool"
        );

        let entity_repo = self.resolve::<EntityRepository>();

        for parent_id in &params.parent_ids {
            entity_repo
                .add_belongs(&params.child_id, parent_id, params.note.as_deref())
                .await
                .map_err(|e: AppError| McpError::from(e))?;
        }

        let response = AddBelongsResult {
            child_id: params.child_id,
            parent_ids: params.parent_ids,
        };

        tracing::info!(
            child = %response.child_id,
            parents = response.parent_ids.len(),
            "Added belongs relationships"
        );

        Response(response).into()
    }

    /// Add RELATED_TO relationships between entities.
    ///
    /// If a note is provided, it will be auto-embedded for semantic search.
    #[tool(description = "Add RELATED_TO relationships between entities.")]
    pub async fn add_related(
        &self,
        Parameters(params): Parameters<AddRelatedParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::info!(
            from = %params.from_id,
            to = ?params.to_ids,
            "Running add_related tool"
        );

        let entity_repo = self.resolve::<EntityRepository>();
        let embedder = self.resolve::<AppEmbedder>();

        // Generate embedding for note if provided
        let embedding =
            if let Some(ref note) = params.note {
                Some(embedder.embed(note).map_err(|e| {
                    McpError::internal_error(format!("Embedding error: {}", e), None)
                })?)
            } else {
                None
            };

        for to_id in &params.to_ids {
            entity_repo
                .add_related(
                    &params.from_id,
                    to_id,
                    params.relation_type.as_deref(),
                    params.note.as_deref(),
                    embedding.as_deref(),
                )
                .await
                .map_err(|e: AppError| McpError::from(e))?;
        }

        let response = AddRelatedResult {
            from_id: params.from_id,
            to_ids: params.to_ids,
        };

        tracing::info!(
            from = %response.from_id,
            to_count = response.to_ids.len(),
            "Added related relationships"
        );

        Response(response).into()
    }

    /// Add a code-level link between entities.
    ///
    /// Valid link types: CALLS, IMPORTS, IMPLEMENTS, INSTANTIATES.
    #[tool(description = "Add a code-level link (CALLS, IMPORTS, IMPLEMENTS, INSTANTIATES).")]
    pub async fn add_link(
        &self,
        Parameters(params): Parameters<AddLinkParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::info!(
            from = %params.from_id,
            to = %params.to_id,
            link_type = %params.link_type,
            "Running add_link tool"
        );

        let entity_repo = self.resolve::<EntityRepository>();

        entity_repo
            .add_link(&params.from_id, &params.to_id, &params.link_type)
            .await
            .map_err(|e: AppError| McpError::from(e))?;

        let response = AddLinkResult {
            from_id: params.from_id,
            to_id: params.to_id,
            link_type: params.link_type,
        };

        tracing::info!(
            from = %response.from_id,
            to = %response.to_id,
            link_type = %response.link_type,
            "Added link"
        );

        Response(response).into()
    }

    /// Remove document references.
    #[tool(description = "Remove document references by ID.")]
    pub async fn remove_references(
        &self,
        Parameters(params): Parameters<RemoveReferencesParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::info!(
            count = params.reference_ids.len(),
            "Running remove_references tool"
        );

        let doc_repo = self.resolve::<DocumentRepository>();

        for ref_id in &params.reference_ids {
            doc_repo
                .delete_reference(ref_id)
                .await
                .map_err(|e: AppError| McpError::from(e))?;
        }

        let response = RemoveReferencesResult {
            removed_count: params.reference_ids.len(),
            reference_ids: params.reference_ids,
        };

        tracing::info!(count = response.removed_count, "Removed references");

        Response(response).into()
    }
}
