//! Graph abstraction layer for backend-agnostic database access.
//!
//! This module provides a trait-based abstraction over graph databases,
//! enabling the same application code to work with different backends
//! (PostgreSQL + Apache AGE, SQLite + graphqlite, etc.).
//!
//! # Architecture
//!
//! The abstraction is built on a hierarchy of traits:
//!
//! - [`CypherExecutor`] - Execute Cypher queries (required for all graph backends)
//! - [`SqlExecutor`] - Execute SQL queries (optional, for backends that support it)
//! - [`Transaction`] - Transaction lifecycle (commit/rollback)
//! - [`GraphClient`] - Connection management and transaction creation
//!
//! # Usage
//!
//! ```ignore
//! use gnapsis::graph::{Graph, GraphClient, QueryExt};
//!
//! // Create a graph instance with any backend
//! let graph = Graph::new(client);
//!
//! // Simple query
//! let rows = graph.query("MATCH (n:Entity) RETURN n")
//!     .fetch_all()
//!     .await?;
//!
//! // Query with parameters
//! let rows = graph.query("MATCH (n:Entity) WHERE n.id = $id RETURN n")
//!     .param("id", entity_id)
//!     .execute()
//!     .await?;
//!
//! // Write query (no results)
//! graph.query("CREATE (n:Entity {id: $id, name: $name})")
//!     .param("id", new_id)
//!     .param("name", name)
//!     .run()
//!     .await?;
//! ```

mod macros;
mod query;
mod row;
mod traits;

pub mod backends;

// Re-export core types
pub use query::{Query, QueryExt};
pub use row::{Params, Row, RowStream};
pub use traits::{CypherExecutor, GraphClient, SqlExecutor, Transaction};

// Re-export macro (defined at crate root via #[macro_export])
#[doc(inline)]
pub use crate::cypher;

// --- Graph wrapper struct ---

use std::future::Future;

use crate::error::AppError;

/// High-level wrapper providing a convenient API for graph operations.
///
/// `Graph` wraps any [`GraphClient`] and provides:
/// - Direct queries (auto-commit per query)
/// - Transaction closures (user controls commit/rollback)
///
/// # Example
///
/// ```ignore
/// let graph = Graph::new(client);
///
/// // Direct query
/// let rows = graph.query("MATCH (n) RETURN n").fetch_all().await?;
///
/// // Transaction with closure - user must call commit()
/// let result = graph.transaction(|txn| async move {
///     txn.query("CREATE (a:Node {id: $id})").param("id", "a").run().await?;
///     txn.query("CREATE (b:Node {id: $id})").param("id", "b").run().await?;
///     txn.commit().await?;
///     Ok(())
/// }).await?;
/// ```
pub struct Graph<C: GraphClient> {
    client: C,
}

impl<C: GraphClient> Graph<C> {
    /// Creates a new graph wrapper around the given client.
    pub fn new(client: C) -> Self {
        Self { client }
    }

    /// Returns a reference to the underlying client.
    pub fn client(&self) -> &C {
        &self.client
    }

    /// Consumes the wrapper and returns the underlying client.
    pub fn into_inner(self) -> C {
        self.client
    }

    /// Creates a query builder for a direct (auto-commit) query.
    ///
    /// Each query executes in its own implicit transaction.
    pub fn query(&self, cypher: &str) -> Query<'_, C> {
        Query::new(&self.client, cypher)
    }

    /// Executes a closure within a transaction.
    ///
    /// The closure receives the transaction and is responsible for
    /// calling `commit()` or `rollback()`. If the closure returns
    /// without committing, any changes will be rolled back when the
    /// transaction is dropped (backend-dependent behavior).
    ///
    /// # Arguments
    ///
    /// * `f` - A closure that receives the transaction and returns a future
    ///
    /// # Returns
    ///
    /// The result of the closure.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let count = graph.transaction(|txn| async move {
    ///     txn.query("CREATE (n:Node {id: $id})").param("id", "1").run().await?;
    ///     txn.query("CREATE (n:Node {id: $id})").param("id", "2").run().await?;
    ///
    ///     let rows = txn.query("MATCH (n:Node) RETURN count(n) as count")
    ///         .fetch_all()
    ///         .await?;
    ///     let count: i64 = rows[0].get("count")?;
    ///
    ///     txn.commit().await?;
    ///     Ok(count)
    /// }).await?;
    /// ```
    pub async fn transaction<F, R, Fut>(&self, f: F) -> Result<R, AppError>
    where
        F: FnOnce(C::Tx<'_>) -> Fut,
        Fut: Future<Output = Result<R, AppError>>,
    {
        let txn = self.client.begin().await?;
        f(txn).await
    }
}

// Forward CypherExecutor to the underlying client for convenience
#[async_trait::async_trait]
impl<C: GraphClient> CypherExecutor for Graph<C> {
    async fn execute_cypher(
        &self,
        cypher: &str,
        params: Params,
    ) -> Result<RowStream<'_>, AppError> {
        self.client.execute_cypher(cypher, params).await
    }

    async fn run_cypher(&self, cypher: &str, params: Params) -> Result<(), AppError> {
        self.client.run_cypher(cypher, params).await
    }
}
