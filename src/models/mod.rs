//! Domain models for the knowledge graph.

mod category;
mod document;
mod entity;
mod scope;

pub use category::Category;
pub use document::{ContentType, Document, DocumentReference};
pub use entity::{generate_ulid, Entity};
pub use scope::Scope;
