//! Graph models for query results and entity context.

use serde::{Deserialize, Serialize};

use super::{Entity, Reference};

/// Category classification with scope information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryClassification {
    /// Category ID.
    pub id: String,
    /// Category name.
    pub name: String,
    /// Scope name (Domain, Feature, Namespace, Component, Unit).
    pub scope: String,
}

/// Entity with full context including classifications, references, and hierarchy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityWithContext {
    /// The entity itself.
    pub entity: Entity,
    /// Category classifications.
    pub classifications: Vec<CategoryClassification>,
    /// Document references.
    pub references: Vec<Reference>,
    /// Parent entities (via BELONGS_TO).
    pub parents: Vec<Entity>,
    /// Child entities (via BELONGS_TO inward).
    pub children: Vec<Entity>,
    /// Related entities (via RELATED_TO).
    pub related: Vec<Entity>,
}

/// Entity with its document reference for document queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityWithReference {
    /// The entity.
    pub entity: Entity,
    /// The reference linking entity to the document.
    pub reference: Reference,
}

/// A search result with similarity score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult<T> {
    /// The matched item.
    pub item: T,
    /// Similarity score (0.0 to 1.0).
    pub score: f32,
}

// ============================================================================
// Semantic Query Graph (Budget-Aware BFS Results)
// ============================================================================

/// A node in the semantic query graph - either an Entity or a DocumentReference.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QueryGraphNode {
    /// An entity node.
    Entity {
        /// Entity ID.
        id: String,
        /// Entity name.
        name: String,
        /// Entity description.
        description: String,
        /// Scope (if classified).
        #[serde(skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
        /// Semantic relevance to the query (0.0 to 1.0).
        relevance: f32,
    },
    /// A document reference node (code or text).
    Reference {
        /// Reference ID.
        id: String,
        /// Path to the document.
        document_path: String,
        /// Starting line number (1-indexed).
        start_line: u32,
        /// Ending line number (1-indexed).
        end_line: u32,
        /// Description of what this reference points to.
        description: String,
        /// Relevance inherited from parent entity.
        relevance: f32,
    },
}

/// An edge in the semantic query graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryGraphEdge {
    /// Source node ID.
    pub from_id: String,
    /// Target node ID.
    pub to_id: String,
    /// Relationship type (BELONGS_TO, RELATED_TO, CALLS, etc.).
    pub relationship: String,
    /// Optional note on the relationship.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Relevance score when this edge was traversed.
    pub relevance: f32,
}

/// Summary of an entity for query results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryEntitySummary {
    /// Entity ID.
    pub id: String,
    /// Entity name.
    pub name: String,
    /// Entity description.
    pub description: String,
    /// Scope (if classified).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// Category (if classified).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
}

/// Summary of an entity for project overview.
/// Includes parent info for hierarchy display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectEntitySummary {
    /// Entity ID.
    pub id: String,
    /// Entity name.
    pub name: String,
    /// Entity description.
    pub description: String,
    /// Category (if classified).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Parent entity ID (for hierarchy).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
}

/// Statistics about the semantic query execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryGraphStats {
    /// Number of nodes visited during BFS.
    pub nodes_visited: usize,
    /// Number of nodes pruned (budget or relevance).
    pub nodes_pruned: usize,
    /// Estimated token count of included nodes.
    pub estimated_tokens: usize,
}

/// Result of a semantic subgraph query using Best-First Search.
///
/// Contains relevance-scored nodes and edges within token/node budgets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryGraph {
    /// The starting/root entity.
    pub root_entity: QueryEntitySummary,
    /// Nodes included in the result (sorted by relevance).
    pub nodes: Vec<QueryGraphNode>,
    /// Edges between included nodes.
    pub edges: Vec<QueryGraphEdge>,
    /// Query execution statistics.
    pub stats: QueryGraphStats,
}
