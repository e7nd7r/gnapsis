//! Business logic services for the knowledge graph.
//!
//! Services orchestrate repositories and handle business rules,
//! using the `FromContext` derive macro for dependency injection.

mod graph;
mod validation;

pub use graph::GraphService;
pub use validation::{ValidationIssue, ValidationService};
