//! Application context providing dependency injection root.

use raggy::embeddings::FastEmbedProvider;
use raggy::Embedder;
use std::sync::Arc;

use crate::config::Config;
use crate::di::Context as ContextDerive;
use crate::graph::backends::postgres::PostgresClient;
use crate::graph::Graph;
use crate::nvim::LazyNvimClient;

/// Type alias for the graph client used throughout the application.
pub type AppGraph = Graph<PostgresClient>;

/// Type alias for the embedder used throughout the application.
pub type AppEmbedder = Arc<Embedder<FastEmbedProvider>>;

/// Root application context for dependency injection.
///
/// The Context holds all shared dependencies and uses `#[derive(Context)]`
/// to generate `FromRef` implementations for each field, enabling
/// compile-time dependency resolution.
#[derive(ContextDerive, Clone)]
pub struct Context {
    /// PostgreSQL + Apache AGE graph database client.
    pub graph: AppGraph,
    /// Application configuration.
    pub config: Arc<Config>,
    /// Embedding provider for semantic search.
    pub embedder: AppEmbedder,
    /// Lazy-loaded Neovim client for LSP and visualization.
    pub nvim: LazyNvimClient,
}

impl Context {
    /// Creates a new context with the given dependencies.
    pub fn new(
        graph: Graph<PostgresClient>,
        config: Config,
        embedder: Embedder<FastEmbedProvider>,
    ) -> Self {
        Self {
            graph,
            config: Arc::new(config),
            embedder: Arc::new(embedder),
            nvim: LazyNvimClient::new(),
        }
    }
}
