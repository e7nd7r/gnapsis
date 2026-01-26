//! Core traits for graph database abstraction.
//!
//! This module defines the trait hierarchy that backends must implement:
//!
//! - [`CypherExecutor`] - Required for all graph backends
//! - [`SqlExecutor`] - Optional, for backends that support SQL
//! - [`Transaction`] - Transaction lifecycle management
//! - [`GraphClient`] - Connection pool and transaction creation

use async_trait::async_trait;

use crate::error::AppError;
use crate::graph::row::{Params, RowStream};

/// Executes Cypher queries against a graph database.
///
/// This is the core trait that all graph backends must implement.
/// It provides methods for executing queries that return results
/// and queries that don't (mutations).
#[async_trait]
pub trait CypherExecutor: Send + Sync {
    /// Executes a Cypher query and returns a stream of result rows.
    ///
    /// Use this for queries that return data (MATCH, RETURN).
    ///
    /// # Arguments
    ///
    /// * `cypher` - The Cypher query string
    /// * `params` - Parameters to bind to the query
    ///
    /// # Returns
    ///
    /// A stream of rows that can be iterated asynchronously.
    async fn execute_cypher(&self, cypher: &str, params: Params)
        -> Result<RowStream<'_>, AppError>;

    /// Executes a Cypher query without returning results.
    ///
    /// Use this for mutations (CREATE, MERGE, DELETE, SET).
    ///
    /// # Arguments
    ///
    /// * `cypher` - The Cypher query string
    /// * `params` - Parameters to bind to the query
    async fn run_cypher(&self, cypher: &str, params: Params) -> Result<(), AppError>;
}

/// Executes SQL queries against the database.
///
/// This trait is optional - only backends that support SQL (like PostgreSQL)
/// need to implement it. It's useful for DDL operations and backend-specific
/// functionality that can't be expressed in Cypher.
#[async_trait]
pub trait SqlExecutor: Send + Sync {
    /// Executes a SQL statement without returning results.
    ///
    /// Use this for DDL (CREATE TABLE, CREATE INDEX) and other
    /// non-query operations.
    async fn execute_sql(&self, sql: &str) -> Result<(), AppError>;

    /// Executes a SQL query and returns a stream of result rows.
    async fn query_sql(&self, sql: &str) -> Result<RowStream<'_>, AppError>;
}

/// Transaction lifecycle management.
///
/// This trait handles committing or rolling back a transaction.
/// It's separate from the executor traits to allow flexibility
/// in how transactions are composed.
#[async_trait]
pub trait Transaction: Send + Sync {
    /// Commits the transaction, making all changes permanent.
    ///
    /// Consumes the transaction - it cannot be used after commit.
    async fn commit(self) -> Result<(), AppError>;

    /// Rolls back the transaction, discarding all changes.
    ///
    /// Consumes the transaction - it cannot be used after rollback.
    async fn rollback(self) -> Result<(), AppError>;
}

/// A graph database client that can begin transactions.
///
/// This trait extends [`CypherExecutor`] to add transaction support.
/// Implementations typically wrap a connection pool and provide
/// auto-commit queries via the executor methods, plus explicit
/// transactions via [`begin`](GraphClient::begin).
#[async_trait]
pub trait GraphClient: CypherExecutor {
    /// The transaction type returned by this client.
    type Tx<'a>: Transaction + CypherExecutor
    where
        Self: 'a;

    /// Begins a new transaction.
    ///
    /// The returned transaction can be used to execute queries,
    /// then must be either committed or rolled back.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let txn = client.begin().await?;
    /// txn.run_cypher("CREATE (n:Node {id: $id})", params).await?;
    /// txn.commit().await?;
    /// ```
    async fn begin(&self) -> Result<Self::Tx<'_>, AppError>;
}
