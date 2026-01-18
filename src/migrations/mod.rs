//! Schema migrations for Neo4j with version tracking.
//!
//! Migrations are:
//! - **Idempotent**: Use `IF NOT EXISTS`, `MERGE`, `COALESCE` - required for safe retries
//! - **Additive-only**: Never delete properties, nodes, relationships, or constraints
//! - **Forward-only**: No rollback support - create compensating migrations if needed
//! - **Version-tracked**: Schema version stored in graph metadata node
//! - **Auto-applied**: Migrations run automatically on `init_project`
//!
//! Note: Neo4j doesn't allow mixing schema changes (CREATE CONSTRAINT/INDEX) with
//! data writes (MERGE/CREATE) in the same transaction. Migrations must be idempotent.

mod m001_schema;
mod m002_triggers;
mod m003_seed_data;

pub use m001_schema::M001Schema;
pub use m002_triggers::M002Triggers;
pub use m003_seed_data::M003SeedData;

use async_trait::async_trait;
use neo4rs::{query, Graph, Txn};

use crate::error::AppError;

/// A schema migration.
///
/// Migrations are applied in order of their version number.
/// Each migration runs within a transaction - on failure, changes are rolled back.
/// Migrations must be idempotent for safe retries.
#[async_trait(?Send)]
pub trait Migration: Sync {
    /// Unique identifier for this migration (e.g., "m001_init").
    fn id(&self) -> &'static str;

    /// Version number - migrations run in ascending order.
    fn version(&self) -> u32;

    /// Human-readable description for logging.
    fn description(&self) -> &'static str;

    /// Apply the migration within a transaction.
    async fn up(&self, txn: &mut Txn) -> Result<(), AppError>;
}

/// Result of running migrations.
#[derive(Debug, Clone)]
pub struct MigrationResult {
    /// Schema version before migrations ran.
    pub previous_version: u32,
    /// Schema version after migrations ran.
    pub current_version: u32,
    /// List of migration IDs that were applied.
    pub applied_migrations: Vec<String>,
}

/// Returns all migrations in version order.
fn all_migrations() -> Vec<Box<dyn Migration>> {
    vec![
        Box::new(M001Schema),
        Box::new(M002Triggers),
        Box::new(M003SeedData),
    ]
}

/// Run all pending migrations.
///
/// Migrations are applied in version order. Only migrations with a version
/// higher than the current schema version are applied. Each migration runs
/// in its own transaction - on failure, changes are rolled back. The schema
/// version is updated after each successful migration.
pub async fn run_migrations(graph: &Graph) -> Result<MigrationResult, AppError> {
    let previous_version = get_schema_version(graph).await?;
    let migrations = all_migrations();

    let mut applied = vec![];
    let mut current_version = previous_version;

    for migration in &migrations {
        if migration.version() > current_version {
            tracing::info!(
                "Applying migration {} (v{}): {}",
                migration.id(),
                migration.version(),
                migration.description()
            );

            // Run migration in a transaction
            let mut txn = graph.start_txn().await?;
            match migration.up(&mut txn).await {
                Ok(()) => {
                    txn.commit().await?;
                }
                Err(e) => {
                    tracing::error!("Migration {} failed, rolling back: {}", migration.id(), e);
                    txn.rollback().await?;
                    return Err(e);
                }
            }

            // Update version after successful commit
            update_schema_version(graph, migration.version(), migration.id()).await?;
            current_version = migration.version();
            applied.push(migration.id().to_string());
        }
    }

    Ok(MigrationResult {
        previous_version,
        current_version,
        applied_migrations: applied,
    })
}

/// Get the current schema version from the database.
///
/// Returns 0 if no schema version node exists (fresh database).
async fn get_schema_version(graph: &Graph) -> Result<u32, AppError> {
    let mut result = graph
        .execute(query(
            "MATCH (sv:SchemaVersion) RETURN sv.version AS version LIMIT 1",
        ))
        .await?;

    if let Some(row) = result.next().await? {
        let version: i64 = row.get("version").map_err(|e| AppError::Query {
            message: e.to_string(),
            query: "get schema version".to_string(),
        })?;
        Ok(version as u32)
    } else {
        Ok(0)
    }
}

/// Update the schema version after applying a migration.
async fn update_schema_version(
    graph: &Graph,
    version: u32,
    migration_id: &str,
) -> Result<(), AppError> {
    graph
        .run(
            query(
                "MERGE (sv:SchemaVersion)
                 SET sv.version = $version,
                     sv.applied_migrations = coalesce(sv.applied_migrations, []) + [$migration_id],
                     sv.last_applied_at = datetime()",
            )
            .param("version", version as i64)
            .param("migration_id", migration_id),
        )
        .await?;
    Ok(())
}
