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
        tracing::info!(
            "Loaded configuration for project: {:?}",
            config.project.name
        );

        // Connect to PostgreSQL + AGE
        tracing::info!("Connecting to PostgreSQL at {}", config.postgres.uri);
        let client = PostgresClient::connect(&config.postgres.uri, &config.postgres.graph_name)
            .await
            .map_err(|e| color_eyre::eyre::eyre!("Failed to connect: {}", e))?;
        tracing::info!("Connected to PostgreSQL + AGE");

        // Run migrations
        tracing::info!("Running migrations...");
        let result = run_migrations(&client)
            .await
            .map_err(|e| color_eyre::eyre::eyre!("Migration failed: {}", e))?;

        if result.applied_migrations.is_empty() {
            tracing::info!(
                "Database already at version {}, no migrations needed",
                result.current_version
            );
        } else {
            tracing::info!(
                "Migrations complete: v{} -> v{}, applied: {:?}",
                result.previous_version,
                result.current_version,
                result.applied_migrations
            );
        }

        Ok(())
    }
}
