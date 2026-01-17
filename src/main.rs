//! Gnapsis - Code Intelligence Graph MCP Server

pub mod config;
pub mod context;
pub mod di;
pub mod error;
pub mod models;
pub mod tools;

// Re-export FromRef at crate root for di-macros generated code
pub use di::FromRef;

use clap::Parser;

#[derive(Parser)]
#[command(name = "gnapsis")]
#[command(about = "Code intelligence graph - MCP server for knowledge management")]
struct Cli {
    /// Run in verbose mode
    #[arg(short, long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;

    let cli = Cli::parse();

    // Initialize logging
    let filter = if cli.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt().with_env_filter(filter).init();

    tracing::info!("Starting Gnapsis MCP server");

    // TODO: Initialize MCP server

    Ok(())
}
