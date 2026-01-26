//! Macro for convenient Cypher query construction.

/// Macro for inline Cypher queries with optional parameters.
///
/// This macro provides a shorthand for creating and parameterizing queries.
///
/// # Usage
///
/// ```ignore
/// use gnapsis::graph::cypher;
///
/// // Query without parameters
/// let query = cypher!(graph, "MATCH (n) RETURN n");
///
/// // Query with parameters
/// let query = cypher!(graph, "MATCH (n) WHERE n.id = $id RETURN n", id = entity_id);
///
/// // Multiple parameters
/// let query = cypher!(
///     graph,
///     "MATCH (n) WHERE n.id = $id AND n.name = $name RETURN n",
///     id = entity_id,
///     name = entity_name
/// );
///
/// // Execute the query
/// let rows = query.fetch_all().await?;
/// ```
#[macro_export]
macro_rules! cypher {
    // Query without parameters
    ($graph:expr, $query:expr) => {
        $graph.query($query)
    };
    // Query with parameters
    ($graph:expr, $query:expr, $($name:ident = $value:expr),+ $(,)?) => {
        $graph.query($query)$(.param(stringify!($name), $value))+
    };
}

#[cfg(test)]
mod tests {
    use crate::error::AppError;
    use crate::graph::query::QueryExt;
    use crate::graph::row::Params;
    use crate::graph::traits::CypherExecutor;

    struct TestExecutor;

    #[async_trait::async_trait]
    impl CypherExecutor for TestExecutor {
        async fn execute_cypher(
            &self,
            _cypher: &str,
            _params: Params,
        ) -> Result<crate::graph::row::RowStream<'_>, AppError> {
            Ok(Box::pin(futures::stream::empty()))
        }

        async fn run_cypher(&self, _cypher: &str, _params: Params) -> Result<(), AppError> {
            Ok(())
        }
    }

    #[test]
    fn test_cypher_macro_no_params() {
        let executor = TestExecutor;
        let _query = cypher!(executor, "MATCH (n) RETURN n");
        // Just verify it compiles
    }

    #[test]
    fn test_cypher_macro_with_params() {
        let executor = TestExecutor;
        let id = "test-id";
        let count = 42;
        let _query = cypher!(
            executor,
            "MATCH (n) WHERE n.id = $id RETURN n LIMIT $count",
            id = id,
            count = count
        );
        // Just verify it compiles
    }

    #[test]
    fn test_cypher_macro_trailing_comma() {
        let executor = TestExecutor;
        let id = "test-id";
        let _query = cypher!(executor, "MATCH (n) WHERE n.id = $id RETURN n", id = id,);
        // Just verify it compiles with trailing comma
    }
}
