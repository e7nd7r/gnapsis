//! Init command handler.

use color_eyre::Result;

use crate::config::Config;
use crate::graph::backends::postgres::PostgresClient;
use crate::migrations::run_migrations;

use super::App;

impl App {
    /// Run the init command to initialize the database schema.
    pub async fn run_init(&self) -> Result<()> {
        // Load configuration
        let config = Config::load()?;
        let graph_name = config.project.graph_name();
        tracing::info!(
            "Loaded configuration for project: {} (graph: {})",
            config.project.name,
            graph_name
        );

        // Connect to PostgreSQL + AGE
        tracing::info!("Connecting to PostgreSQL at {}", config.postgres.uri);
        let client = PostgresClient::connect(&config.postgres.uri, &graph_name)
            .await
            .map_err(|e| color_eyre::eyre::eyre!("Failed to connect: {}", e))?;
        tracing::info!("Connected to PostgreSQL + AGE");

        // Ensure graph exists (creates if not present)
        tracing::info!("Ensuring graph '{}' exists...", graph_name);
        client
            .ensure_graph_exists()
            .await
            .map_err(|e| color_eyre::eyre::eyre!("Failed to create graph: {}", e))?;

        // Run migrations
        tracing::info!("Running migrations...");
        let result = run_migrations(&client, &graph_name)
            .await
            .map_err(|e| color_eyre::eyre::eyre!("Migration failed: {}", e))?;

        let no_migrations =
            result.applied_db_migrations.is_empty() && result.applied_graph_migrations.is_empty();

        if no_migrations {
            tracing::info!(
                "Database already at db_version={}, graph_version={}, no migrations needed",
                result.db_version,
                result.graph_version
            );
        } else {
            if !result.applied_db_migrations.is_empty() {
                tracing::info!(
                    "DB migrations complete: v{}, applied: {:?}",
                    result.db_version,
                    result.applied_db_migrations
                );
            }
            if !result.applied_graph_migrations.is_empty() {
                tracing::info!(
                    "Graph migrations complete: v{}, applied: {:?}",
                    result.graph_version,
                    result.applied_graph_migrations
                );
            }
        }

        Ok(())
    }
}
