//! Migration traits and registry.

use futures::future::BoxFuture;

use crate::error::AppError;
use crate::graph::{CypherExecutor, GraphClient, SqlExecutor, Transaction as _};

// =============================================================================
// Migration Contexts
// =============================================================================

pub trait GraphMigrationContext: CypherExecutor + SqlExecutor + Send + Sync {}
impl<T: CypherExecutor + SqlExecutor + Send + Sync> GraphMigrationContext for T {}

// =============================================================================
// Migration Trait
// =============================================================================

/// Base migration trait with explicit lifetime-bounded future.
/// Uses BoxFuture to avoid `'static` requirements from `#[async_trait]`.
pub trait Migration: Send + Sync {
    type Context: ?Sized + Sync;

    fn id(&self) -> &'static str;
    fn version(&self) -> u32;
    fn description(&self) -> &'static str;
    fn up<'a>(&'a self, ctx: &'a Self::Context) -> BoxFuture<'a, Result<(), AppError>>;
}

pub trait DbMigration: Migration<Context = dyn SqlExecutor + Sync> {}
impl<T: Migration<Context = dyn SqlExecutor + Sync>> DbMigration for T {}

pub trait GraphMigration: Migration<Context = dyn GraphMigrationContext + Sync> {
    fn graph_name(&self) -> &str;
}

// =============================================================================
// Migration Registry
// =============================================================================

pub struct Register<T: ?Sized> {
    migrations: Vec<Box<T>>,
}

// DbMigration register
impl Register<dyn DbMigration> {
    pub fn new() -> Self {
        Self {
            migrations: Vec::new(),
        }
    }

    pub fn register(mut self, migration: impl DbMigration + 'static) -> Self {
        self.migrations.push(Box::new(migration));
        self
    }

    /// Iterate over migrations.
    pub fn iter(&self) -> impl Iterator<Item = &dyn DbMigration> {
        self.migrations.iter().map(|m| m.as_ref())
    }

    /// Run all pending migrations above `current_version`.
    /// Each migration runs in its own transaction.
    /// Returns (new_version, applied_migration_ids).
    pub async fn run_pending<C>(
        &self,
        client: &C,
        current_version: u32,
    ) -> Result<(u32, Vec<String>), AppError>
    where
        C: GraphClient + 'static,
        for<'a> C::Tx<'a>: SqlExecutor + 'static,
    {
        let mut applied = vec![];
        let mut new_version = current_version;

        for migration in &self.migrations {
            if migration.version() <= current_version {
                continue;
            }

            tracing::info!(
                "Applying DB migration {} (v{}): {}",
                migration.id(),
                migration.version(),
                migration.description()
            );

            let txn = client.begin().await?;
            match migration.up(&txn).await {
                Ok(()) => txn.commit().await?,
                Err(e) => {
                    tracing::error!("DB migration {} failed: {}", migration.id(), e);
                    txn.rollback().await?;
                    return Err(e);
                }
            }

            new_version = migration.version();
            applied.push(migration.id().to_string());
        }

        Ok((new_version, applied))
    }
}

impl Default for Register<dyn DbMigration> {
    fn default() -> Self {
        Self::new()
    }
}

// GraphMigration register
impl Register<dyn GraphMigration> {
    pub fn new() -> Self {
        Self {
            migrations: Vec::new(),
        }
    }

    pub fn register(mut self, migration: impl GraphMigration + 'static) -> Self {
        self.migrations.push(Box::new(migration));
        self
    }

    /// Iterate over migrations.
    pub fn iter(&self) -> impl Iterator<Item = &dyn GraphMigration> {
        self.migrations.iter().map(|m| m.as_ref())
    }

    /// Run all pending migrations above `current_version`.
    /// Each migration runs in its own transaction.
    /// Returns (new_version, applied_migration_ids).
    pub async fn run_pending<C>(
        &self,
        client: &C,
        current_version: u32,
    ) -> Result<(u32, Vec<String>), AppError>
    where
        C: GraphClient + 'static,
        for<'a> C::Tx<'a>: CypherExecutor + SqlExecutor + 'static,
    {
        let mut applied = vec![];
        let mut new_version = current_version;

        for migration in &self.migrations {
            if migration.version() <= current_version {
                continue;
            }

            tracing::info!(
                "Applying graph migration {} (v{}) on '{}': {}",
                migration.id(),
                migration.version(),
                migration.graph_name(),
                migration.description()
            );

            let txn = client.begin().await?;
            match migration.up(&txn).await {
                Ok(()) => txn.commit().await?,
                Err(e) => {
                    tracing::error!("Graph migration {} failed: {}", migration.id(), e);
                    txn.rollback().await?;
                    return Err(e);
                }
            }

            new_version = migration.version();
            applied.push(migration.id().to_string());
        }

        Ok((new_version, applied))
    }
}

impl Default for Register<dyn GraphMigration> {
    fn default() -> Self {
        Self::new()
    }
}
