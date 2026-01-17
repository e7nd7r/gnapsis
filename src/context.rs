//! Application context providing dependency injection root.

use neo4rs::Graph;
use std::sync::Arc;

use crate::config::Config;
use crate::di::Context as ContextDerive;

/// Root application context for dependency injection.
///
/// The Context holds all shared dependencies and uses `#[derive(Context)]`
/// to generate `FromRef` implementations for each field, enabling
/// compile-time dependency resolution.
#[derive(ContextDerive, Clone)]
pub struct Context {
    /// Neo4j graph database connection pool.
    pub graph: Arc<Graph>,
    /// Application configuration.
    pub config: Arc<Config>,
}

impl Context {
    /// Creates a new context with the given dependencies.
    pub fn new(graph: Graph, config: Config) -> Self {
        Self {
            graph: Arc::new(graph),
            config: Arc::new(config),
        }
    }
}
