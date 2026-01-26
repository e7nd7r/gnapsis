//! Query builder for fluent Cypher query construction.

use futures::TryStreamExt;
use serde::Serialize;
use serde_json::Value as JsonValue;

use crate::error::AppError;
use crate::graph::row::{Params, Row, RowStream};
use crate::graph::traits::CypherExecutor;

/// A builder for constructing and executing Cypher queries.
///
/// `Query` provides a fluent API for adding parameters and executing
/// queries against any [`CypherExecutor`].
///
/// # Example
///
/// ```ignore
/// let rows = Query::new(&client, "MATCH (n:Entity) WHERE n.id = $id RETURN n")
///     .param("id", "entity-123")
///     .fetch_all()
///     .await?;
/// ```
pub struct Query<'a, E: CypherExecutor + ?Sized> {
    executor: &'a E,
    cypher: String,
    params: Params,
}

impl<'a, E: CypherExecutor + ?Sized> Query<'a, E> {
    /// Creates a new query builder.
    ///
    /// # Arguments
    ///
    /// * `executor` - The executor to run the query against
    /// * `cypher` - The Cypher query string
    pub fn new(executor: &'a E, cypher: &str) -> Self {
        Self {
            executor,
            cypher: cypher.to_string(),
            params: Params::new(),
        }
    }

    /// Adds a parameter to the query.
    ///
    /// Parameters are referenced in Cypher using `$name` syntax.
    ///
    /// # Arguments
    ///
    /// * `name` - The parameter name (without the $ prefix)
    /// * `value` - The parameter value (must be serializable)
    ///
    /// # Panics
    ///
    /// Panics if the value cannot be serialized to JSON.
    pub fn param<T: Serialize>(mut self, name: &str, value: T) -> Self {
        let json_value = serde_json::to_value(value).expect("failed to serialize parameter value");
        self.params.insert(name.to_string(), json_value);
        self
    }

    /// Adds a parameter that's already a JSON value.
    ///
    /// Use this when you already have a `serde_json::Value`.
    pub fn param_raw(mut self, name: &str, value: JsonValue) -> Self {
        self.params.insert(name.to_string(), value);
        self
    }

    /// Executes the query and returns a stream of rows.
    ///
    /// Use this for memory-efficient iteration over large result sets.
    /// Rows are fetched on-demand.
    pub async fn execute(self) -> Result<RowStream<'a>, AppError> {
        self.executor
            .execute_cypher(&self.cypher, self.params)
            .await
    }

    /// Executes the query and collects all rows into a vector.
    ///
    /// This is a convenience method for small result sets. For large
    /// results, use [`execute`](Query::execute) and stream the rows.
    pub async fn fetch_all(self) -> Result<Vec<Row>, AppError> {
        self.execute().await?.try_collect().await
    }

    /// Executes the query and returns the first row, if any.
    pub async fn fetch_one(self) -> Result<Option<Row>, AppError> {
        let mut stream = self.execute().await?;
        use futures::StreamExt;
        stream.next().await.transpose()
    }

    /// Executes the query without returning results.
    ///
    /// Use this for mutations (CREATE, MERGE, DELETE, SET).
    pub async fn run(self) -> Result<(), AppError> {
        self.executor.run_cypher(&self.cypher, self.params).await
    }
}

/// Extension trait providing a convenient `query()` method.
///
/// This trait is automatically implemented for all [`CypherExecutor`]
/// types, allowing you to write `executor.query("...")` instead of
/// `Query::new(&executor, "...")`.
pub trait QueryExt: CypherExecutor {
    /// Creates a new query builder for this executor.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use gnapsis::graph::QueryExt;
    ///
    /// let rows = client.query("MATCH (n) RETURN n")
    ///     .param("limit", 10)
    ///     .fetch_all()
    ///     .await?;
    /// ```
    fn query(&self, cypher: &str) -> Query<'_, Self>
    where
        Self: Sized,
    {
        Query::new(self, cypher)
    }
}

// Blanket implementation for all CypherExecutor types
impl<E: CypherExecutor> QueryExt for E {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // Mock executor for testing
    struct MockExecutor {
        expected_cypher: String,
        expected_params: Params,
    }

    #[async_trait::async_trait]
    impl CypherExecutor for MockExecutor {
        async fn execute_cypher(
            &self,
            cypher: &str,
            params: Params,
        ) -> Result<RowStream<'_>, AppError> {
            assert_eq!(cypher, self.expected_cypher);
            assert_eq!(params, self.expected_params);
            // Return empty stream
            Ok(Box::pin(futures::stream::empty()))
        }

        async fn run_cypher(&self, cypher: &str, params: Params) -> Result<(), AppError> {
            assert_eq!(cypher, self.expected_cypher);
            assert_eq!(params, self.expected_params);
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_query_no_params() {
        let executor = MockExecutor {
            expected_cypher: "MATCH (n) RETURN n".to_string(),
            expected_params: HashMap::new(),
        };

        let result = executor.query("MATCH (n) RETURN n").fetch_all().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_query_with_params() {
        let mut expected_params = HashMap::new();
        expected_params.insert("id".to_string(), serde_json::json!("test-id"));
        expected_params.insert("count".to_string(), serde_json::json!(42));

        let executor = MockExecutor {
            expected_cypher: "MATCH (n) WHERE n.id = $id RETURN n LIMIT $count".to_string(),
            expected_params,
        };

        let result = executor
            .query("MATCH (n) WHERE n.id = $id RETURN n LIMIT $count")
            .param("id", "test-id")
            .param("count", 42)
            .fetch_all()
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_query_run() {
        let mut expected_params = HashMap::new();
        expected_params.insert("id".to_string(), serde_json::json!("new-id"));

        let executor = MockExecutor {
            expected_cypher: "CREATE (n:Node {id: $id})".to_string(),
            expected_params,
        };

        let result = executor
            .query("CREATE (n:Node {id: $id})")
            .param("id", "new-id")
            .run()
            .await;
        assert!(result.is_ok());
    }
}
