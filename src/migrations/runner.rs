//! Migration runner with version tracking.

use futures::TryStreamExt;

use crate::error::AppError;
use crate::graph::{CypherExecutor, GraphClient, Query, SqlExecutor, Transaction};
use crate::migrations::db;
use crate::migrations::graph;

/// Result of running migrations.
#[derive(Debug, Clone)]
pub struct MigrationResult {
    pub db_version: u32,
    pub graph_version: u32,
    pub applied_db_migrations: Vec<String>,
    pub applied_graph_migrations: Vec<String>,
}

/// Run all pending migrations (both database and graph).
pub async fn run_migrations<C>(client: &C, graph_name: &str) -> Result<MigrationResult, AppError>
where
    C: GraphClient + 'static,
    for<'a> C::Tx<'a>: CypherExecutor + SqlExecutor + 'static,
{
    let (db_version, applied_db) = run_db_migrations(client).await?;
    let (graph_version, applied_graph) = run_graph_migrations(client, graph_name).await?;

    Ok(MigrationResult {
        db_version,
        graph_version,
        applied_db_migrations: applied_db,
        applied_graph_migrations: applied_graph,
    })
}

// =============================================================================
// Database Migrations
// =============================================================================

async fn run_db_migrations<C>(client: &C) -> Result<(u32, Vec<String>), AppError>
where
    C: GraphClient + 'static,
    for<'a> C::Tx<'a>: SqlExecutor + 'static,
{
    ensure_db_schema_version_table(client).await?;

    let current_version = get_db_schema_version(client).await?;
    let register = db::create_register();

    let (new_version, applied) = register.run_pending(client, current_version).await?;

    // Update version tracking for each applied migration
    for migration_id in &applied {
        // Find the version for this migration
        let version = register
            .iter()
            .find(|m| m.id() == migration_id)
            .map(|m| m.version())
            .unwrap_or(new_version);
        update_db_schema_version(client, version, migration_id).await?;
    }

    Ok((new_version, applied))
}

// =============================================================================
// Graph Migrations
// =============================================================================

async fn run_graph_migrations<C>(
    client: &C,
    graph_name: &str,
) -> Result<(u32, Vec<String>), AppError>
where
    C: GraphClient + 'static,
    for<'a> C::Tx<'a>: CypherExecutor + SqlExecutor + 'static,
{
    ensure_graph_schema_version(client).await?;

    let current_version = get_graph_schema_version(client).await?;
    let register = graph::create_register(graph_name);

    let (new_version, applied) = register.run_pending(client, current_version).await?;

    // Update version tracking for each applied migration
    for migration_id in &applied {
        let version = register
            .iter()
            .find(|m| m.id() == migration_id)
            .map(|m| m.version())
            .unwrap_or(new_version);
        update_graph_schema_version(client, version, migration_id).await?;
    }

    Ok((new_version, applied))
}

// =============================================================================
// DB Schema Version (SQL table)
// =============================================================================

const CREATE_DB_SCHEMA_VERSION_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS db_schema_version (
    id INTEGER PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    version INTEGER NOT NULL DEFAULT 0,
    applied_migrations TEXT[] NOT NULL DEFAULT '{}',
    last_applied_at TIMESTAMPTZ DEFAULT NOW()
);
INSERT INTO db_schema_version (id, version) VALUES (1, 0) ON CONFLICT (id) DO NOTHING;
"#;

async fn ensure_db_schema_version_table<C>(client: &C) -> Result<(), AppError>
where
    C: GraphClient,
    for<'a> C::Tx<'a>: SqlExecutor,
{
    let txn = client.begin().await?;
    txn.execute_sql(CREATE_DB_SCHEMA_VERSION_TABLE).await?;
    txn.commit().await?;
    Ok(())
}

async fn get_db_schema_version<C>(client: &C) -> Result<u32, AppError>
where
    C: GraphClient,
    for<'a> C::Tx<'a>: SqlExecutor,
{
    let txn = client.begin().await?;
    let rows: Vec<_> = txn
        .query_sql("SELECT version FROM db_schema_version WHERE id = 1")
        .await?
        .try_collect()
        .await?;
    txn.commit().await?;

    Ok(rows
        .first()
        .and_then(|r| r.get::<i64>("version").ok())
        .unwrap_or(0) as u32)
}

async fn update_db_schema_version<C>(
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
        "UPDATE db_schema_version SET version = {}, applied_migrations = array_append(applied_migrations, '{}'), last_applied_at = NOW() WHERE id = 1",
        version, migration_id
    );
    txn.execute_sql(&sql).await?;
    txn.commit().await?;
    Ok(())
}

// =============================================================================
// Graph Schema Version (:SchemaVersion node)
// =============================================================================

async fn ensure_graph_schema_version<C>(client: &C) -> Result<(), AppError>
where
    C: GraphClient,
    for<'a> C::Tx<'a>: CypherExecutor,
{
    let now = chrono::Utc::now().to_rfc3339();
    let txn = client.begin().await?;

    // Check if SchemaVersion node exists
    let rows = Query::new(
        &txn,
        "MATCH (sv:SchemaVersion {id: 'schema_version'}) RETURN sv",
    )
    .fetch_all()
    .await?;

    if rows.is_empty() {
        // Create it if it doesn't exist (AGE doesn't support ON CREATE SET)
        Query::new(
            &txn,
            "CREATE (sv:SchemaVersion {id: 'schema_version', version: 0, applied_migrations: [], created_at: $now, last_applied_at: $now})",
        )
        .param("now", &now)
        .run()
        .await?;
    }

    txn.commit().await?;
    Ok(())
}

async fn get_graph_schema_version<C>(client: &C) -> Result<u32, AppError>
where
    C: GraphClient,
    for<'a> C::Tx<'a>: CypherExecutor,
{
    let txn = client.begin().await?;
    let rows = Query::new(
        &txn,
        "MATCH (sv:SchemaVersion {id: 'schema_version'}) RETURN sv.version as version",
    )
    .fetch_all()
    .await?;
    txn.commit().await?;

    Ok(rows
        .first()
        .and_then(|r| r.get::<i64>("version").ok())
        .unwrap_or(0) as u32)
}

async fn update_graph_schema_version<C>(
    client: &C,
    version: u32,
    migration_id: &str,
) -> Result<(), AppError>
where
    C: GraphClient,
    for<'a> C::Tx<'a>: CypherExecutor,
{
    let now = chrono::Utc::now().to_rfc3339();
    let txn = client.begin().await?;
    Query::new(
        &txn,
        "MATCH (sv:SchemaVersion {id: 'schema_version'})
         SET sv.version = $version, sv.applied_migrations = sv.applied_migrations + [$migration_id], sv.last_applied_at = $now",
    )
    .param("version", version as i64)
    .param("migration_id", migration_id)
    .param("now", &now)
    .run()
    .await?;
    txn.commit().await?;
    Ok(())
}
