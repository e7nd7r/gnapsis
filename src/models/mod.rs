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
    CategoryClassification, CompositionGraph, CompositionNode, EntityWithContext,
    EntityWithReference, SearchResult, Subgraph, SubgraphEdge, SubgraphNode,
};
pub use scope::Scope;
