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
use crate::models::{generate_ulid, ContentType, Entity};
use crate::repositories::{
    CategoryRepository, CreateReferenceParams, DocumentRepository, EntityRepository,
};

// ============================================================================
// Parameter Types
// ============================================================================

/// Parameters for create_entity tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateEntityParams {
    /// Human-readable name for the entity.
    pub name: String,
    /// Detailed description (auto-embedded for semantic search).
    pub description: String,
}

/// Parameters for update_entity tool.
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

/// Document reference input for add_references tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct DocumentRefInput {
    /// Path to the document (relative to repo root).
    pub document_path: String,
    /// Starting line number (1-indexed).
    pub start_line: u32,
    /// Ending line number (1-indexed).
    pub end_line: u32,
    /// Description of what this reference points to (auto-embedded).
    pub description: String,
    /// Content type: "code:rust", "code:typescript", "markdown", etc.
    #[serde(default)]
    pub content_type: Option<String>,
    /// Character offset within the file (optional).
    #[serde(default)]
    pub offset: Option<u32>,
    /// Git commit SHA (defaults to HEAD).
    #[serde(default)]
    pub commit_sha: Option<String>,
    /// LSP symbol name (optional).
    #[serde(default)]
    pub lsp_symbol: Option<String>,
    /// LSP symbol kind (optional).
    #[serde(default)]
    pub lsp_kind: Option<i32>,
    /// LSP range as string (optional).
    #[serde(default)]
    pub lsp_range: Option<String>,
}

/// Parameters for add_references tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct AddReferencesParams {
    /// Entity ID to add references to.
    pub entity_id: String,
    /// Document references to add.
    pub refs: Vec<DocumentRefInput>,
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
    pub has_embedding: bool,
}

impl From<Entity> for EntityResult {
    fn from(e: Entity) -> Self {
        Self {
            id: e.id,
            name: e.name,
            description: e.description,
            has_embedding: e.embedding.is_some(),
        }
    }
}

/// Response for create_entity tool.
#[derive(Debug, Serialize)]
pub struct CreateEntityResult {
    pub entity: EntityResult,
}

/// Response for update_entity tool.
#[derive(Debug, Serialize)]
pub struct UpdateEntityResult {
    pub entity: EntityResult,
    pub embedding_updated: bool,
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

/// Document reference result.
#[derive(Debug, Serialize)]
pub struct DocumentRefResult {
    pub id: String,
    pub document_path: String,
    pub start_line: u32,
    pub end_line: u32,
}

/// Response for add_references tool.
#[derive(Debug, Serialize)]
pub struct AddReferencesResult {
    pub entity_id: String,
    pub references: Vec<DocumentRefResult>,
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
    /// The description is automatically embedded for semantic search.
    #[tool(description = "Create a new entity with auto-embedding of description.")]
    pub async fn create_entity(
        &self,
        Parameters(params): Parameters<CreateEntityParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::info!(name = %params.name, "Running create_entity tool");

        let entity_repo = self.resolve::<EntityRepository>();
        let embedder = self.resolve::<AppEmbedder>();

        // Generate embedding for description
        let embedding = embedder
            .embed(&params.description)
            .map_err(|e| McpError::internal_error(format!("Embedding error: {}", e), None))?;

        let entity = Entity {
            id: generate_ulid(),
            name: params.name,
            description: params.description,
            embedding: Some(embedding),
            created_at: chrono::Utc::now(),
        };

        let created = entity_repo
            .create(&entity)
            .await
            .map_err(|e: AppError| McpError::from(e))?;

        let response = CreateEntityResult {
            entity: created.into(),
        };

        tracing::info!(id = %response.entity.id, "Created entity");

        Response(response).into()
    }

    /// Update an entity's name and/or description.
    ///
    /// If description changes, the embedding is regenerated.
    #[tool(description = "Update an entity. Re-embeds if description changes.")]
    pub async fn update_entity(
        &self,
        Parameters(params): Parameters<UpdateEntityParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::info!(id = %params.entity_id, "Running update_entity tool");

        let entity_repo = self.resolve::<EntityRepository>();
        let embedder = self.resolve::<AppEmbedder>();

        // Generate new embedding if description is being updated
        let new_embedding =
            if let Some(ref desc) = params.description {
                Some(embedder.embed(desc).map_err(|e| {
                    McpError::internal_error(format!("Embedding error: {}", e), None)
                })?)
            } else {
                None
            };

        let updated = entity_repo
            .update(
                &params.entity_id,
                params.name.as_deref(),
                params.description.as_deref(),
                new_embedding.as_deref(),
            )
            .await
            .map_err(|e: AppError| McpError::from(e))?;

        let embedding_updated = new_embedding.is_some();
        let response = UpdateEntityResult {
            entity: updated.into(),
            embedding_updated,
        };

        tracing::info!(
            id = %response.entity.id,
            embedding_updated = embedding_updated,
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

        for to_id in &params.to_ids {
            entity_repo
                .add_related(&params.from_id, to_id, params.relation_type.as_deref())
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

    /// Add document references to an entity.
    ///
    /// Descriptions are auto-embedded for semantic search.
    #[tool(description = "Add document references to an entity with auto-embedding.")]
    pub async fn add_references(
        &self,
        Parameters(params): Parameters<AddReferencesParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::info!(
            entity_id = %params.entity_id,
            count = params.refs.len(),
            "Running add_references tool"
        );

        let doc_repo = self.resolve::<DocumentRepository>();
        let embedder = self.resolve::<AppEmbedder>();

        let mut created_refs = Vec::new();

        for ref_input in params.refs {
            // Generate embedding for description
            let embedding = embedder
                .embed(&ref_input.description)
                .map_err(|e| McpError::internal_error(format!("Embedding error: {}", e), None))?;

            // Parse content type
            let content_type = ref_input
                .content_type
                .as_ref()
                .map(|ct| {
                    if ct.starts_with("code:") {
                        ContentType::Code(ct.strip_prefix("code:").unwrap().to_string())
                    } else if ct == "markdown" {
                        ContentType::Markdown
                    } else {
                        ContentType::Code(ct.clone())
                    }
                })
                .unwrap_or(ContentType::Markdown);

            let doc_ref = doc_repo
                .create_reference(CreateReferenceParams {
                    entity_id: &params.entity_id,
                    document_path: &ref_input.document_path,
                    start_line: ref_input.start_line,
                    end_line: ref_input.end_line,
                    offset: ref_input.offset,
                    commit_sha: ref_input.commit_sha.as_deref().unwrap_or("HEAD"),
                    content_type: &content_type,
                    description: &ref_input.description,
                    embedding: Some(&embedding),
                    lsp_symbol: ref_input.lsp_symbol.as_deref(),
                    lsp_kind: ref_input.lsp_kind,
                    lsp_range: ref_input.lsp_range.as_deref(),
                })
                .await
                .map_err(|e: AppError| McpError::from(e))?;

            created_refs.push(DocumentRefResult {
                id: doc_ref.id,
                document_path: doc_ref.document_path,
                start_line: doc_ref.start_line,
                end_line: doc_ref.end_line,
            });
        }

        let response = AddReferencesResult {
            entity_id: params.entity_id,
            references: created_refs,
        };

        tracing::info!(
            entity_id = %response.entity_id,
            count = response.references.len(),
            "Added references"
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
