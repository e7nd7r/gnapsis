//! Entity model representing nodes in the knowledge graph.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

/// An entity in the knowledge graph.
///
/// Entities are the primary nodes representing code concepts,
/// documentation, or any other knowledge artifacts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    /// Unique identifier (ULID).
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Detailed description of the entity.
    pub description: String,
    /// Vector embedding for semantic search (internal, not serialized).
    #[serde(skip_serializing)]
    pub embedding: Option<Vec<f32>>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
}

impl Entity {
    /// Creates a new entity with a generated ULID and current timestamp.
    pub fn new(name: String, description: String) -> Self {
        Self {
            id: generate_ulid(),
            name,
            description,
            embedding: None,
            created_at: Utc::now(),
        }
    }
}

/// Generates a new ULID string.
pub fn generate_ulid() -> String {
    Ulid::new().to_string()
}
