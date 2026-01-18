//! Init command handler.

use color_eyre::Result;
use neo4rs::Graph;

use crate::config::Config;
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

        // Connect to Neo4j
        tracing::info!("Connecting to Neo4j at {}", config.neo4j.uri);
        let graph = Graph::new(
            &config.neo4j.uri,
            &config.neo4j.user,
            config.neo4j.password.as_deref().unwrap_or(""),
        )
        .await?;
        tracing::info!("Connected to Neo4j");

        // Run migrations
        tracing::info!("Running migrations...");
        let result = run_migrations(&graph).await?;

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
