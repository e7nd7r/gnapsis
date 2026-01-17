//! Category model for entity classification.

use serde::{Deserialize, Serialize};

use super::Scope;

/// A category for classifying entities at a specific scope level.
///
/// Categories provide the classification values within each scope.
/// For example, at the Component scope, categories might include
/// "class", "struct", "trait", "interface", etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Category {
    /// Unique identifier (ULID).
    pub id: String,
    /// Category name (e.g., "class", "function", "module").
    pub name: String,
    /// The scope level this category belongs to.
    pub scope: Scope,
    /// Optional description of what this category represents.
    pub description: Option<String>,
}
