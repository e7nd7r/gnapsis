//! Data access layer for Neo4j graph operations.
//!
//! Repositories provide a clean abstraction over Neo4j queries,
//! using the `FromContext` derive macro for dependency injection.

mod category;
mod document;
mod entity;
mod schema;

pub use category::CategoryRepository;
pub use document::{CreateReferenceParams, DocumentRepository, UpdateReferenceParams};
pub use entity::EntityRepository;
pub use schema::{ProjectStats, SchemaRepository, ScopeInfo};
