//! Gnapsis - Code Intelligence Graph MCP Server

pub mod cli;
pub mod config;
pub mod context;
pub mod di;
pub mod error;
pub mod git;
pub mod mcp;
pub mod migrations;
pub mod models;
pub mod repositories;
pub mod services;
pub mod visualization;

// Re-export FromRef at crate root for di-macros generated code
pub use di::FromRef;

use clap::Parser;

use crate::cli::App;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;

    let app = App::parse();

    // Initialize logging to stderr (stdout is for MCP protocol)
    let filter = if app.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();

    app.run().await
}
