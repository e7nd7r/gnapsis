//! Schema migrations for PostgreSQL + Apache AGE with version tracking.
//!
//! Migrations are:
//! - **Idempotent**: Use `IF NOT EXISTS`, `MERGE`, `COALESCE` - required for safe retries
//! - **Additive-only**: Never delete properties, nodes, relationships, or constraints
//! - **Forward-only**: No rollback support - create compensating migrations if needed
//! - **Version-tracked**: Schema version stored in `schema_version` SQL table
//! - **Auto-applied**: Migrations run automatically on `init_project`
//!
//! Migrations can use both Cypher (via `CypherExecutor`) and SQL (via `SqlExecutor`)
//! depending on what each migration needs.

mod m001_schema;
mod m002_seed_data;
mod m003_ontology_v2;
mod m004_ontology_v2_data;

pub use m001_schema::M001Schema;
pub use m002_seed_data::M002SeedData;
pub use m003_ontology_v2::M003OntologyV2;
pub use m004_ontology_v2_data::M004OntologyV2Data;

use crate::error::AppError;
use crate::graph::{CypherExecutor, GraphClient, SqlExecutor, Transaction};

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

/// All migrations in version order.
///
/// Using a const array instead of trait objects since the generic `up<T>` method
/// makes the Migration trait not object-safe. This is fine since we have a fixed
/// set of migrations known at compile time.
const MIGRATIONS: &[MigrationEntry] = &[
    MigrationEntry {
        id: "m001_schema",
        version: 1,
        description: "Schema setup (graph creation, indexes)",
    },
    MigrationEntry {
        id: "m002_seed_data",
        version: 2,
        description: "Seed data (scopes and default categories)",
    },
    MigrationEntry {
        id: "m003_ontology_v2",
        version: 3,
        description: "Ontology V2 schema (CodeReference and TextReference indexes)",
    },
    MigrationEntry {
        id: "m004_ontology_v2_data",
        version: 4,
        description: "Migrate DocumentReference nodes to CodeReference/TextReference",
    },
];

/// Migration metadata entry.
struct MigrationEntry {
    id: &'static str,
    version: u32,
    description: &'static str,
}

/// Dispatches to the appropriate migration implementation.
async fn run_migration<T>(id: &str, txn: &T) -> Result<(), AppError>
where
    T: CypherExecutor + SqlExecutor + Sync,
{
    match id {
        "m001_schema" => M001Schema.up(txn).await,
        "m002_seed_data" => M002SeedData.up(txn).await,
        "m003_ontology_v2" => M003OntologyV2.up(txn).await,
        "m004_ontology_v2_data" => M004OntologyV2Data.up(txn).await,
        _ => Err(AppError::Internal(format!("Unknown migration: {}", id))),
    }
}

/// Run all pending migrations.
///
/// Migrations are applied in version order. Only migrations with a version
/// higher than the current schema version are applied. Each migration runs
/// in its own transaction - on failure, changes are rolled back. The schema
/// version is updated after each successful migration.
///
/// # Type Parameters
///
/// * `C` - A graph client that can begin transactions supporting both Cypher and SQL
pub async fn run_migrations<C>(client: &C) -> Result<MigrationResult, AppError>
where
    C: GraphClient,
    for<'a> C::Tx<'a>: CypherExecutor + SqlExecutor,
{
    // Ensure schema_version table exists (outside transaction for DDL)
    ensure_schema_version_table(client).await?;

    let previous_version = get_schema_version(client).await?;

    let mut applied = vec![];
    let mut current_version = previous_version;

    for migration in MIGRATIONS {
        if migration.version > current_version {
            tracing::info!(
                "Applying migration {} (v{}): {}",
                migration.id,
                migration.version,
                migration.description
            );

            // Run migration in a transaction
            let txn = client.begin().await?;
            match run_migration(migration.id, &txn).await {
                Ok(()) => {
                    txn.commit().await?;
                }
                Err(e) => {
                    tracing::error!("Migration {} failed, rolling back: {}", migration.id, e);
                    txn.rollback().await?;
                    return Err(e);
                }
            }

            // Update version after successful commit (separate transaction)
            update_schema_version(client, migration.version, migration.id).await?;
            current_version = migration.version;
            applied.push(migration.id.to_string());
        }
    }

    Ok(MigrationResult {
        previous_version,
        current_version,
        applied_migrations: applied,
    })
}

/// SQL to create the schema_version table.
const CREATE_SCHEMA_VERSION_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS schema_version (
    id INTEGER PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    version INTEGER NOT NULL DEFAULT 0,
    applied_migrations TEXT[] NOT NULL DEFAULT '{}',
    last_applied_at TIMESTAMPTZ DEFAULT NOW()
);

-- Ensure exactly one row exists
INSERT INTO schema_version (id, version)
VALUES (1, 0)
ON CONFLICT (id) DO NOTHING;
"#;

/// Ensures the schema_version table exists.
///
/// This runs outside a transaction since DDL in PostgreSQL can cause issues
/// when mixed with other operations in the same transaction.
async fn ensure_schema_version_table<C>(client: &C) -> Result<(), AppError>
where
    C: GraphClient,
    for<'a> C::Tx<'a>: SqlExecutor,
{
    let txn = client.begin().await?;
    txn.execute_sql(CREATE_SCHEMA_VERSION_TABLE).await?;
    txn.commit().await?;
    Ok(())
}

/// Get the current schema version from the database.
///
/// Returns 0 if no version has been set (fresh database).
async fn get_schema_version<C>(client: &C) -> Result<u32, AppError>
where
    C: GraphClient,
    for<'a> C::Tx<'a>: SqlExecutor,
{
    use futures::TryStreamExt;

    let txn = client.begin().await?;
    let rows: Vec<_> = txn
        .query_sql("SELECT version FROM schema_version WHERE id = 1")
        .await?
        .try_collect()
        .await?;

    let version = if let Some(row) = rows.first() {
        row.get::<i64>("version").unwrap_or(0) as u32
    } else {
        0
    };

    txn.commit().await?;
    Ok(version)
}

/// Update the schema version after applying a migration.
async fn update_schema_version<C>(
    client: &C,
    version: u32,
    migration_id: &str,
) -> Result<(), AppError>
where
    C: GraphClient,
    for<'a> C::Tx<'a>: SqlExecutor,
{
    let txn = client.begin().await?;
    let sql = format!(
        "UPDATE schema_version
         SET version = {},
             applied_migrations = array_append(applied_migrations, '{}'),
             last_applied_at = NOW()
         WHERE id = 1",
        version, migration_id
    );
    txn.execute_sql(&sql).await?;
    txn.commit().await?;
    Ok(())
}
