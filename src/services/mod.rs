//! Business logic services for the knowledge graph.
//!
//! Services orchestrate repositories and handle business rules,
//! using the `FromContext` derive macro for dependency injection.

mod commands;
mod graph;
mod validation;

pub use commands::{
    AttachedEntityInfo, CommandOutcome, CommandResult, CommandService, EntityCommand,
    ExecutedCommand, FailedCommand, FailureContext, LinkType, NewReference,
};
pub use graph::GraphService;
pub use validation::{ValidationIssue, ValidationService};
