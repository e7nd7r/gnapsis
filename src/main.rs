//! Gnapsis - Code Intelligence Graph MCP Server

pub mod config;
pub mod context;
pub mod di;
pub mod error;
pub mod migrations;
pub mod models;
pub mod repositories;
pub mod tools;

// Re-export FromRef at crate root for di-macros generated code
pub use di::FromRef;

use clap::Parser;
use neo4rs::Graph;

use crate::config::Config;
use crate::migrations::run_migrations;

#[derive(Parser)]
#[command(name = "gnapsis")]
#[command(about = "Code intelligence graph - MCP server for knowledge management")]
struct Cli {
    /// Run in verbose mode
    #[arg(short, long)]
    verbose: bool,

    /// Initialize the database schema
    #[arg(long)]
    init: bool,
}

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;

    let cli = Cli::parse();

    // Initialize logging
    let filter = if cli.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt().with_env_filter(filter).init();

    tracing::info!("Starting Gnapsis MCP server");

    // Load configuration
    let config = Config::load()?;
    tracing::info!(
        "Loaded configuration for project: {:?}",
        config.project.name
    );

    // Connect to Neo4j
    tracing::info!("Connecting to Neo4j at {}", config.neo4j.uri);
    let graph = Graph::new(
        &config.neo4j.uri,
        &config.neo4j.user,
        config.neo4j.password.as_deref().unwrap_or(""),
    )
    .await?;
    tracing::info!("Connected to Neo4j");

    // Run migrations if --init flag is set
    if cli.init {
        tracing::info!("Running migrations...");
        let result = run_migrations(&graph).await?;
        tracing::info!(
            "Migrations complete: v{} -> v{}, applied: {:?}",
            result.previous_version,
            result.current_version,
            result.applied_migrations
        );
    }

    // TODO: Initialize MCP server

    Ok(())
}
