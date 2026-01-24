//! MCP server command handler.

use color_eyre::Result;
use rmcp::ServiceExt;

use crate::config::Config;
use crate::context::Context;
use crate::mcp::McpServer;

use super::App;

impl App {
    /// Run the MCP server with stdio transport.
    pub async fn run_mcp(&self) -> Result<()> {
        tracing::info!("Starting Gnapsis MCP server");

        let config = Config::load()?;
        let ctx = Context::from(config).await?;
        let server = McpServer::new(ctx);

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
