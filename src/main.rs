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
pub mod nvim;
pub mod repositories;
pub mod services;
pub mod visualization;

// Re-export FromRef at crate root for di-macros generated code
pub use di::FromRef;

use clap::Parser;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::cli::App;

/// Get the log directory path (~/.gnapsis/logs).
fn log_dir() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|h| h.join(".gnapsis").join("logs"))
}

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;

    let app = App::parse();

    // Set up file logging to ~/.gnapsis/logs
    let log_file = log_dir().and_then(|dir| {
        std::fs::create_dir_all(&dir).ok()?;
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("gnapsis.log"))
            .ok()
    });

    let filter = if app.verbose { "debug" } else { "info" };

    // Check if RUST_LOG is explicitly set (for Docker/container environments)
    let rust_log_set = std::env::var("RUST_LOG").is_ok();

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter));

    let registry = tracing_subscriber::registry().with(env_filter);

    // If RUST_LOG is set, always output to stderr (for Docker/containers)
    // Otherwise, write to file if possible
    if rust_log_set {
        let stderr_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stderr);
        registry.with(stderr_layer).init();
    } else if let Some(file) = log_file {
        let file_layer = tracing_subscriber::fmt::layer()
            .with_writer(std::sync::Mutex::new(file))
            .with_ansi(false);
        registry.with(file_layer).init();
    } else {
        let stderr_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stderr);
        registry.with(stderr_layer).init();
    }

    app.run().await
}
