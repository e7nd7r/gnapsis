//! Domain models for the knowledge graph.

mod category;
mod document;
mod entity;
mod graph;
mod scope;

pub use category::Category;
pub use document::{CodeReference, Document, Reference, TextReference};
pub use entity::{generate_ulid, Entity};
pub use graph::{
    CategoryClassification, EntityWithContext, EntityWithReference, ProjectEntitySummary,
    QueryEntitySummary, QueryGraph, QueryGraphEdge, QueryGraphNode, QueryGraphStats, SearchResult,
};
pub use scope::Scope;
