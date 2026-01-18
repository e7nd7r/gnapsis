//! MCP server command handler.

use color_eyre::Result;
use neo4rs::Graph;
use rmcp::ServiceExt;

use crate::config::Config;
use crate::context::Context;
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

        // Connect to Neo4j
        tracing::debug!("Connecting to Neo4j at {}", config.neo4j.uri);
        let graph = Graph::new(
            &config.neo4j.uri,
            &config.neo4j.user,
            config.neo4j.password.as_deref().unwrap_or(""),
        )
        .await?;
        tracing::debug!("Connected to Neo4j");

        // Create context and server
        let ctx = Context::new(graph, config);
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
}
