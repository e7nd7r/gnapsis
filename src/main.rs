//! Gnapsis - Code Intelligence Graph MCP Server

use clap::Parser;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use gnapsis::cli::App;

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
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter));

    let registry = tracing_subscriber::registry().with(env_filter);

    if let Some(file) = log_file {
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
