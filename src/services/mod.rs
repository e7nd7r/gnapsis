//! Business logic services for the knowledge graph.
//!
//! Services orchestrate repositories and handle business rules,
//! using the `FromContext` derive macro for dependency injection.

mod graph;

pub use graph::GraphService;
