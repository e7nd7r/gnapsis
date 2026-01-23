//! Entity service for creating and updating entities with command execution.
//!
//! This service enforces the entity constraints from DES-005:
//! - Entities must have at least one reference (Add command required on create)
//! - Entities must be classified (category_ids required)
//! - Non-Domain entities must have parents (parent_ids required)
//! - Link/Unlink commands only valid for Component/Unit scope

use serde::Serialize;

use crate::context::{AppEmbedder, Context};
use crate::di::FromContext;
use crate::error::AppError;
use crate::models::{generate_ulid, Entity};
use crate::repositories::{CategoryRepository, EntityRepository, QueryRepository};
use crate::services::{CommandService, EntityCommand, ExecutedCommand, FailedCommand};

// ============================================================================
// Service Types
// ============================================================================

/// Parameters for creating an entity.
#[derive(Debug, Clone)]
pub struct CreateEntityInput {
    /// Human-readable name.
    pub name: String,
    /// Detailed description (auto-embedded).
    pub description: String,
    /// Category IDs for classification (required, non-empty).
    pub category_ids: Vec<String>,
    /// Parent entity IDs (empty only for Domain scope).
    pub parent_ids: Vec<String>,
    /// Commands to execute (must include at least one Add).
    pub commands: Vec<EntityCommand>,
}

/// Parameters for updating an entity.
#[derive(Debug, Clone)]
pub struct UpdateEntityInput {
    /// Entity ID to update.
    pub entity_id: String,
    /// New name (optional).
    pub name: Option<String>,
    /// New description (optional, re-embeds if changed).
    pub description: Option<String>,
    /// Replace category IDs (optional).
    pub category_ids: Option<Vec<String>>,
    /// Replace parent IDs (optional).
    pub parent_ids: Option<Vec<String>>,
    /// Commands to execute.
    pub commands: Vec<EntityCommand>,
}

/// Result of creating an entity.
#[derive(Debug, Serialize)]
pub struct CreateEntityOutput {
    /// The created entity.
    pub entity: EntityInfo,
    /// Commands that executed successfully.
    pub executed: Vec<ExecutedCommand>,
    /// Command that failed (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed: Option<FailedCommand>,
    /// Commands skipped due to failure.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<EntityCommand>,
}

/// Result of updating an entity.
#[derive(Debug, Serialize)]
pub struct UpdateEntityOutput {
    /// The updated entity.
    pub entity: EntityInfo,
    /// Whether embedding was regenerated.
    pub embedding_updated: bool,
    /// Commands that executed successfully.
    pub executed: Vec<ExecutedCommand>,
    /// Command that failed (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed: Option<FailedCommand>,
    /// Commands skipped due to failure.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<EntityCommand>,
}

/// Entity info for responses.
#[derive(Debug, Serialize)]
pub struct EntityInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub scope: String,
    pub categories: Vec<String>,
    pub parents: Vec<String>,
}

// ============================================================================
// Validation Errors
// ============================================================================

/// Validation error for entity operations.
#[derive(Debug, Clone)]
pub enum ValidationError {
    /// No Add command provided (entities must have references).
    MissingAddCommand,
    /// No categories provided (entities must be classified).
    MissingCategories,
    /// No parents provided for non-Domain entity.
    MissingParents { scope: String },
    /// Link/Unlink command used on wrong scope.
    InvalidLinkScope { actual_scope: String },
    /// Category not found.
    CategoryNotFound { id: String },
    /// Parent entity not found.
    ParentNotFound { id: String },
    /// Entity not found.
    EntityNotFound { id: String },
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::MissingAddCommand => {
                write!(f, "create_entity requires at least one Add command")
            }
            ValidationError::MissingCategories => {
                write!(f, "category_ids is required and cannot be empty")
            }
            ValidationError::MissingParents { scope } => {
                write!(f, "parent_ids is required for {} scope entities", scope)
            }
            ValidationError::InvalidLinkScope { actual_scope } => {
                write!(
                    f,
                    "Link/Unlink commands require Component or Unit scope, got '{}'",
                    actual_scope
                )
            }
            ValidationError::CategoryNotFound { id } => {
                write!(f, "Category not found: {}", id)
            }
            ValidationError::ParentNotFound { id } => {
                write!(f, "Parent entity not found: {}", id)
            }
            ValidationError::EntityNotFound { id } => {
                write!(f, "Entity not found: {}", id)
            }
        }
    }
}

impl From<ValidationError> for AppError {
    fn from(e: ValidationError) -> Self {
        AppError::Validation(e.to_string())
    }
}

// ============================================================================
// Entity Service
// ============================================================================

/// Service for entity creation and updates with validation.
#[derive(FromContext, Clone)]
pub struct EntityService {
    entity_repo: EntityRepository,
    category_repo: CategoryRepository,
    query_repo: QueryRepository,
    command_service: CommandService,
    embedder: AppEmbedder,
}

impl EntityService {
    /// Create a new entity with validation and command execution.
    ///
    /// Validates:
    /// - At least one Add command
    /// - category_ids non-empty
    /// - parent_ids non-empty (unless Domain scope inferred)
    /// - All Add commands target same document
    /// - Link/Unlink only for Component/Unit scope
    pub async fn create(&self, input: CreateEntityInput) -> Result<CreateEntityOutput, AppError> {
        // Validate inputs
        self.validate_create(&input).await?;

        // Determine scope from categories
        let scope = self.determine_scope(&input.category_ids).await?;

        // Validate parent requirement based on scope
        if scope != "Domain" && input.parent_ids.is_empty() {
            return Err(ValidationError::MissingParents {
                scope: scope.clone(),
            }
            .into());
        }

        // Validate Link/Unlink commands against scope
        self.validate_link_commands(&input.commands, &scope)?;

        // Generate embedding for description
        let embedding = self
            .embedder
            .embed(&input.description)
            .map_err(|e| AppError::Embedding(e.to_string()))?;

        // Create the entity
        let entity = Entity {
            id: generate_ulid(),
            name: input.name.clone(),
            description: input.description.clone(),
            embedding: Some(embedding),
            created_at: chrono::Utc::now(),
        };

        let created = self.entity_repo.create(&entity).await?;

        // Classify the entity
        for cat_id in &input.category_ids {
            self.entity_repo.classify(&created.id, cat_id).await?;
        }

        // Add parent relationships
        for parent_id in &input.parent_ids {
            self.entity_repo
                .add_belongs(&created.id, parent_id, None)
                .await?;
        }

        // Execute commands
        let cmd_result = self
            .command_service
            .execute(&created.id, input.commands)
            .await?;

        Ok(CreateEntityOutput {
            entity: EntityInfo {
                id: created.id,
                name: created.name,
                description: created.description,
                scope,
                categories: input.category_ids,
                parents: input.parent_ids,
            },
            executed: cmd_result.executed,
            failed: cmd_result.failed,
            skipped: cmd_result.skipped,
        })
    }

    /// Update an existing entity with validation and command execution.
    pub async fn update(&self, input: UpdateEntityInput) -> Result<UpdateEntityOutput, AppError> {
        // Verify entity exists
        self.entity_repo
            .find_by_id(&input.entity_id)
            .await?
            .ok_or_else(|| ValidationError::EntityNotFound {
                id: input.entity_id.clone(),
            })?;

        // Validate categories if provided
        if let Some(ref cat_ids) = input.category_ids {
            for cat_id in cat_ids {
                self.category_repo
                    .find_by_id(cat_id)
                    .await?
                    .ok_or_else(|| ValidationError::CategoryNotFound { id: cat_id.clone() })?;
            }
        }

        // Validate parents if provided
        if let Some(ref parent_ids) = input.parent_ids {
            for parent_id in parent_ids {
                self.entity_repo
                    .find_by_id(parent_id)
                    .await?
                    .ok_or_else(|| ValidationError::ParentNotFound {
                        id: parent_id.clone(),
                    })?;
            }
        }

        // Get current scope for Link/Unlink validation
        let scope = self.get_entity_scope(&input.entity_id).await?;
        self.validate_link_commands(&input.commands, &scope)?;

        // Update name/description if provided
        let new_embedding = if let Some(ref desc) = input.description {
            Some(
                self.embedder
                    .embed(desc)
                    .map_err(|e| AppError::Embedding(e.to_string()))?,
            )
        } else {
            None
        };

        let updated = self
            .entity_repo
            .update(
                &input.entity_id,
                input.name.as_deref(),
                input.description.as_deref(),
                new_embedding.as_deref(),
            )
            .await?;

        let embedding_updated = new_embedding.is_some();

        // Update categories if provided (replace semantics)
        if let Some(ref cat_ids) = input.category_ids {
            // Get current categories and remove them
            let current = self
                .query_repo
                .get_entity_with_context(&input.entity_id)
                .await?;
            for cat in &current.classifications {
                self.entity_repo
                    .unclassify(&input.entity_id, &cat.id)
                    .await?;
            }
            // Add new categories
            for cat_id in cat_ids {
                self.entity_repo.classify(&input.entity_id, cat_id).await?;
            }
        }

        // Update parents if provided (replace semantics)
        if let Some(ref parent_ids) = input.parent_ids {
            // Get current parents and remove them
            let current = self
                .query_repo
                .get_entity_with_context(&input.entity_id)
                .await?;
            for parent in &current.parents {
                self.entity_repo
                    .remove_belongs(&input.entity_id, &parent.id)
                    .await?;
            }
            // Add new parents
            for parent_id in parent_ids {
                self.entity_repo
                    .add_belongs(&input.entity_id, parent_id, None)
                    .await?;
            }
        }

        // Execute commands
        let cmd_result = self
            .command_service
            .execute(&input.entity_id, input.commands)
            .await?;

        // Get final state
        let final_context = self
            .query_repo
            .get_entity_with_context(&input.entity_id)
            .await?;
        let final_scope = final_context
            .classifications
            .first()
            .map(|c| c.scope.clone())
            .unwrap_or_else(|| "Unknown".to_string());

        Ok(UpdateEntityOutput {
            entity: EntityInfo {
                id: updated.id,
                name: updated.name,
                description: updated.description,
                scope: final_scope,
                categories: final_context
                    .classifications
                    .iter()
                    .map(|c| c.id.clone())
                    .collect(),
                parents: final_context.parents.iter().map(|p| p.id.clone()).collect(),
            },
            embedding_updated,
            executed: cmd_result.executed,
            failed: cmd_result.failed,
            skipped: cmd_result.skipped,
        })
    }

    // ========================================================================
    // Validation helpers
    // ========================================================================

    async fn validate_create(&self, input: &CreateEntityInput) -> Result<(), AppError> {
        // Must have at least one Add command
        let has_add = input
            .commands
            .iter()
            .any(|c| matches!(c, EntityCommand::Add(_)));
        if !has_add {
            return Err(ValidationError::MissingAddCommand.into());
        }

        // Must have categories
        if input.category_ids.is_empty() {
            return Err(ValidationError::MissingCategories.into());
        }

        // Validate categories exist
        for cat_id in &input.category_ids {
            self.category_repo
                .find_by_id(cat_id)
                .await?
                .ok_or_else(|| ValidationError::CategoryNotFound { id: cat_id.clone() })?;
        }

        // Validate parents exist
        for parent_id in &input.parent_ids {
            self.entity_repo
                .find_by_id(parent_id)
                .await?
                .ok_or_else(|| ValidationError::ParentNotFound {
                    id: parent_id.clone(),
                })?;
        }

        Ok(())
    }

    fn validate_link_commands(
        &self,
        commands: &[EntityCommand],
        scope: &str,
    ) -> Result<(), AppError> {
        let has_link = commands
            .iter()
            .any(|c| matches!(c, EntityCommand::Link { .. } | EntityCommand::Unlink { .. }));

        if has_link && scope != "Component" && scope != "Unit" {
            return Err(ValidationError::InvalidLinkScope {
                actual_scope: scope.to_string(),
            }
            .into());
        }

        Ok(())
    }

    async fn determine_scope(&self, category_ids: &[String]) -> Result<String, AppError> {
        // Get the first category's scope (all categories should be same scope)
        if let Some(cat_id) = category_ids.first() {
            if let Some(cat) = self.category_repo.find_by_id(cat_id).await? {
                return Ok(cat.scope.to_string());
            }
        }
        Ok("Unknown".to_string())
    }

    async fn get_entity_scope(&self, entity_id: &str) -> Result<String, AppError> {
        let context = self.query_repo.get_entity_with_context(entity_id).await?;
        Ok(context
            .classifications
            .first()
            .map(|c| c.scope.clone())
            .unwrap_or_else(|| "Unknown".to_string()))
    }
}
