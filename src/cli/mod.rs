//! CLI module for Gnapsis.
//!
//! Subcommands:
//! - `init`: Initialize the database schema
//! - `mcp`: Run the MCP server (stdio transport)
//! - `visualize`: Render a graph in 3D

mod init;
mod mcp;
mod visualize;

use clap::{Parser, Subcommand};

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

    /// Run the MCP server (stdio transport)
    Mcp,

    /// Visualize a graph from JSON file in 3D
    Visualize(VisualizeCommand),
}

impl App {
    /// Run the CLI application.
    pub async fn run(self) -> color_eyre::Result<()> {
        match self.command {
            Command::Init => self.run_init().await,
            Command::Mcp => self.run_mcp().await,
            Command::Visualize(cmd) => cmd.run(),
        }
    }
}
