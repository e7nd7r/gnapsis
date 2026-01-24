//! Application context providing dependency injection root.

use color_eyre::Result;
use neo4rs::Graph;
use raggy::embeddings::{FastEmbedConfig, FastEmbedModel, ProviderConfig};
use raggy::{Embedder, EmbeddingProvider, FastEmbedProvider};
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
    /// Creates a context from configuration, connecting to Neo4j and initializing embeddings.
    pub async fn from(config: Config) -> Result<Self> {
        let graph = Graph::new(
            &config.neo4j.uri,
            &config.neo4j.user,
            config.neo4j.password.as_deref().unwrap_or(""),
        )
        .await?;

        let embedder = Self::create_embedder(&config)?;

        Ok(Self {
            graph: Arc::new(graph),
            config: Arc::new(config),
            embedder: Arc::new(embedder),
            nvim: LazyNvimClient::new(),
        })
    }

    /// Create the embedding provider based on configuration.
    fn create_embedder(config: &Config) -> Result<Embedder<FastEmbedProvider>> {
        let model = match config.embedding.model.as_str() {
            "BAAI/bge-small-en-v1.5" | "bge-small-en-v1.5" => FastEmbedModel::BGESmallENV15,
            "BAAI/bge-base-en-v1.5" | "bge-base-en-v1.5" => FastEmbedModel::BGEBaseENV15,
            "BAAI/bge-large-en-v1.5" | "bge-large-en-v1.5" => FastEmbedModel::BGELargeENV15,
            "all-MiniLM-L6-v2" => FastEmbedModel::AllMiniLML6V2,
            "all-MiniLM-L12-v2" => FastEmbedModel::AllMiniLML12V2,
            "nomic-embed-text-v1" => FastEmbedModel::NomicEmbedTextV1,
            "nomic-embed-text-v1.5" => FastEmbedModel::NomicEmbedTextV15,
            _ => FastEmbedModel::BGESmallENV15,
        };

        let provider_config = ProviderConfig::FastEmbed(FastEmbedConfig {
            model,
            show_download_progress: false,
            cache_dir: None,
        });

        let provider = FastEmbedProvider::new(provider_config)?;
        Ok(Embedder::new(provider))
    }
}
