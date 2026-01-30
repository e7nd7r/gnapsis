//! MCP server command handler.

use color_eyre::Result;
use raggy::embeddings::{FastEmbedConfig, FastEmbedModel, ProviderConfig};
use raggy::{Embedder, EmbeddingProvider, FastEmbedProvider};
use rmcp::ServiceExt;

use crate::config::Config;
use crate::context::Context;
use crate::graph::backends::postgres::PostgresClient;
use crate::graph::Graph;
use crate::mcp::McpServer;

use super::App;

impl App {
    /// Run the MCP server with stdio transport.
    pub async fn run_mcp(&self) -> Result<()> {
        tracing::info!("Starting Gnapsis MCP server");

        // Load configuration
        let config = Config::load()?;
        tracing::debug!(
            "Loaded configuration for project: {:?}",
            config.project.name
        );

        // Connect to PostgreSQL + Apache AGE
        let graph_name = config.project.graph_name();
        tracing::debug!(
            "Connecting to PostgreSQL at {} (graph: {})",
            config.postgres.uri,
            graph_name
        );
        let client = PostgresClient::connect(&config.postgres.uri, &graph_name).await?;
        let graph = Graph::new(client);
        tracing::debug!("Connected to PostgreSQL + AGE");

        // Initialize embedding provider
        tracing::debug!(
            "Initializing embedding provider: {}",
            config.embedding.model
        );
        let embedder = Self::create_embedder(&config)?;
        tracing::debug!("Embedding provider initialized");

        // Create context and server
        let ctx = Context::new(graph, config, embedder);
        let server = McpServer::new(ctx);

        // Serve with stdio transport
        let service = server.serve(rmcp::transport::stdio()).await.map_err(|e| {
            tracing::error!(error = %e, "Failed to start MCP server");
            color_eyre::eyre::eyre!("Failed to start MCP server: {}", e)
        })?;

        tracing::info!("MCP server started, waiting for connections");

        service.waiting().await.map_err(|e| {
            tracing::error!(error = %e, "MCP server error");
            color_eyre::eyre::eyre!("MCP server error: {}", e)
        })?;

        tracing::info!("MCP server shutting down");
        Ok(())
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
            _ => FastEmbedModel::BGESmallENV15, // Default fallback
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
