//! HTTP server command handler.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use color_eyre::Result;
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use tower::ServiceBuilder;

use crate::config::Config;
use crate::context::Context;
use crate::mcp::McpServer;

use super::App;

impl App {
    /// Run the MCP server with HTTP transport.
    pub async fn run_serve(&self, host: &str, port: u16) -> Result<()> {
        tracing::info!("Starting Gnapsis HTTP server");

        let config = Config::load()?;
        let ctx = Context::from(config).await?;

        let service = StreamableHttpService::new(
            move || Ok(McpServer::new(ctx.clone())),
            Arc::new(LocalSessionManager::default()),
            StreamableHttpServerConfig::default(),
        );

        let app = Router::new().fallback_service(ServiceBuilder::new().service(service));

        let addr: SocketAddr = format!("{}:{}", host, port)
            .parse()
            .map_err(|e| color_eyre::eyre::eyre!("Invalid address {}:{}: {}", host, port, e))?;

        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| color_eyre::eyre::eyre!("Failed to bind to {}: {}", addr, e))?;

        tracing::info!("Gnapsis HTTP server listening on http://{}", addr);

        axum::serve(listener, app).await.map_err(|e| {
            tracing::error!(error = %e, "HTTP server error");
            color_eyre::eyre::eyre!("HTTP server error: {}", e)
        })?;

        tracing::info!("HTTP server shutting down");
        Ok(())
    }
}
