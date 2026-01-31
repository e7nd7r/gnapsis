//! Data access layer for graph operations.
//!
//! Repositories provide a clean abstraction over graph queries,
//! using the `FromContext` derive macro for dependency injection.

mod category;
mod document;
mod entity;
mod query;
mod schema;

pub use category::CategoryRepository;
pub use document::{
    CreateCodeReferenceParams, CreateTextReferenceParams, DocumentRepository,
    UpdateCodeReferenceParams, UpdateTextReferenceParams,
};
pub use entity::EntityRepository;
pub use query::{QueryRepository, Subgraph, SubgraphEdge, SubgraphNode};
pub use schema::{ProjectStats, SchemaRepository, ScopeInfo};
