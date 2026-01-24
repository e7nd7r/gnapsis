//! CLI module for Gnapsis.
//!
//! Subcommands:
//! - `init`: Initialize the database schema
//! - `mcp`: Run the MCP server (stdio transport)
//! - `serve`: Run the MCP server (HTTP transport)
//! - `embedding`: Embedding model management
//! - `visualize`: Render a graph in 3D

mod embedding;
mod init;
mod mcp;
mod serve;
mod visualize;

use clap::{Parser, Subcommand};

pub use embedding::EmbeddingCommand;
pub use visualize::VisualizeCommand;

/// Gnapsis - Code Intelligence Graph
#[derive(Parser)]
#[command(name = "gnapsis")]
#[command(about = "Code intelligence graph - MCP server for knowledge management")]
#[command(version)]
pub struct App {
    /// Run in verbose mode
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Initialize the database schema and seed data
    Init,

    /// Run the MCP server (stdio transport for local use)
    Mcp,

    /// Run the MCP server (HTTP transport for remote access)
    Serve {
        /// Host address to bind to
        #[arg(long, default_value = "0.0.0.0")]
        host: String,

        /// Port to listen on
        #[arg(long, default_value = "3000")]
        port: u16,
    },

    /// Embedding model management
    Embedding {
        #[command(subcommand)]
        command: EmbeddingCommand,
    },

    /// Visualize a graph from JSON file in 3D
    Visualize(VisualizeCommand),
}

impl App {
    /// Run the CLI application.
    pub async fn run(self) -> color_eyre::Result<()> {
        match self.command {
            Command::Init => self.run_init().await,
            Command::Mcp => self.run_mcp().await,
            Command::Serve { ref host, port } => self.run_serve(host, port).await,
            Command::Embedding { command } => command.run(),
            Command::Visualize(cmd) => cmd.run(),
        }
    }
}
