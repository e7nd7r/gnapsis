//! Embedding model management commands.

use clap::Subcommand;
use color_eyre::Result;

use crate::config::Config;
use crate::context::Context;

/// Embedding model management subcommands.
#[derive(Subcommand)]
pub enum EmbeddingCommand {
    /// Pre-download the embedding model (for Docker builds)
    Warmup,
}

impl EmbeddingCommand {
    /// Run the embedding subcommand.
    pub fn run(&self) -> Result<()> {
        match self {
            EmbeddingCommand::Warmup => run_warmup(),
        }
    }
}

/// Warm up the embedding model by downloading it to the cache.
fn run_warmup() -> Result<()> {
    let config = Config::load()?;

    println!("Warming up embedding model: {}", config.embedding.model);

    let _embedder = Context::create_embedder(&config, true)?;

    let cache_dir =
        std::env::var("FASTEMBED_CACHE_DIR").unwrap_or_else(|_| ".fastembed_cache".to_string());

    println!("Embedding model ready: {}", config.embedding.model);
    println!("Cache location: {}", cache_dir);

    Ok(())
}
