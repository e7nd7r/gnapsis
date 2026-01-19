//! Graph traversal models for subgraph queries and composition hierarchies.

use serde::{Deserialize, Serialize};

use super::{DocumentReference, Entity};

/// Node in a subgraph traversal - either an Entity or DocumentReference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SubgraphNode {
    /// An entity node in the subgraph.
    Entity {
        /// Entity ID.
        id: String,
        /// Entity name.
        name: String,
        /// Entity description.
        description: String,
        /// Distance from the starting node.
        distance: u32,
        /// Category classification (if any).
        category: Option<String>,
    },
    /// A document reference node in the subgraph.
    DocumentReference {
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
        /// Distance from the starting node.
        distance: u32,
    },
}

/// Edge in a subgraph traversal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubgraphEdge {
    /// Source node ID.
    pub from_id: String,
    /// Target node ID.
    pub to_id: String,
    /// Relationship type (e.g., BELONGS_TO, HAS_REFERENCE, CALLS).
    pub relationship: String,
    /// Optional note on the relationship.
    pub note: Option<String>,
}

/// A complete subgraph with nodes and edges.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subgraph {
    /// All nodes in the subgraph.
    pub nodes: Vec<SubgraphNode>,
    /// All edges in the subgraph.
    pub edges: Vec<SubgraphEdge>,
}

/// Node in a composition hierarchy traversal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositionNode {
    /// Entity ID.
    pub id: String,
    /// Entity name.
    pub name: String,
    /// Depth from the starting entity (positive for ancestors, negative for descendants).
    pub depth: i32,
    /// Category classification (if any).
    pub category: Option<String>,
}

/// Result of a composition graph query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositionGraph {
    /// The starting entity.
    pub entity: CompositionNode,
    /// Ancestor entities (via BELONGS_TO outward).
    pub ancestors: Vec<CompositionNode>,
    /// Descendant entities (via BELONGS_TO inward).
    pub descendants: Vec<CompositionNode>,
}

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
    pub references: Vec<DocumentReference>,
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
    /// The document reference linking entity to the document.
    pub reference: DocumentReference,
}

/// A search result with similarity score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult<T> {
    /// The matched item.
    pub item: T,
    /// Similarity score (0.0 to 1.0).
    pub score: f32,
}
