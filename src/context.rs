//! Application context providing dependency injection root.

use neo4rs::Graph;
use raggy::embeddings::FastEmbedProvider;
use raggy::Embedder;
use std::sync::Arc;

use crate::config::Config;
use crate::di::Context as ContextDerive;
use crate::nvim::LazyNvimClient;

/// Type alias for the embedder used throughout the application.
pub type AppEmbedder = Arc<Embedder<FastEmbedProvider>>;

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
    /// Embedding provider for semantic search.
    pub embedder: AppEmbedder,
    /// Lazy-loaded Neovim client for LSP and visualization.
    pub nvim: LazyNvimClient,
}

impl Context {
    /// Creates a new context with the given dependencies.
    pub fn new(graph: Graph, config: Config, embedder: Embedder<FastEmbedProvider>) -> Self {
        Self {
            graph: Arc::new(graph),
            config: Arc::new(config),
            embedder: Arc::new(embedder),
            nvim: LazyNvimClient::new(),
        }
    }
}
