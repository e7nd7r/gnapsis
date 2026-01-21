//! Data access layer for Neo4j graph operations.
//!
//! Repositories provide a clean abstraction over Neo4j queries,
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
pub use query::QueryRepository;
pub use schema::{ProjectStats, SchemaRepository, ScopeInfo};
