//! Business logic services for the knowledge graph.
//!
//! Services orchestrate repositories and handle business rules,
//! using the `FromContext` derive macro for dependency injection.

mod commands;
mod entity;
mod graph;
mod lsp;
mod validation;

pub use commands::{
    AttachedEntityInfo, CommandOutcome, CommandResult, CommandService, EntityCommand,
    ExecutedCommand, FailedCommand, FailureContext, LinkType, NewReference,
};
pub use entity::{
    CreateEntityInput, CreateEntityOutput, EntityInfo, EntityService, UpdateEntityInput,
    UpdateEntityOutput, ValidationError,
};
pub use graph::{
    EntityMatch, GraphService, ReferenceMatch, ScoringStrategy, SearchTarget, SemanticQueryParams,
    UnifiedSearchResult,
};
pub use lsp::{LspError, LspService, LspSymbol};
pub use validation::{ValidationIssue, ValidationService};
