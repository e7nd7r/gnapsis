//! Integration tests for PostgreSQL + Apache AGE backend.
//!
//! These tests require a running PostgreSQL + AGE instance.
//! Run with: `just db-up && cargo test --features integration --test postgres_integration`

#![cfg(feature = "integration")]

use futures::{StreamExt, TryStreamExt};
use gnapsis::graph::backends::postgres::PostgresClient;
use gnapsis::graph::{
    CypherExecutor, Graph, GraphClient, Params, QueryExt, SqlExecutor, Transaction,
};
use gnapsis::migrations::run_migrations;
use serial_test::serial;

const TEST_CONNECTION: &str = "postgresql://postgres:postgres@localhost:5432/gnapsis_dev";
const TEST_GRAPH: &str = "knowledge_graph";

async fn create_client() -> PostgresClient {
    PostgresClient::connect(TEST_CONNECTION, TEST_GRAPH)
        .await
        .expect("Failed to connect to test database")
}

/// Clean up test data before/after tests
async fn cleanup(client: &PostgresClient) {
    let _ = client
        .run_cypher("MATCH (n:IntegrationTest) DETACH DELETE n", Params::new())
        .await;
}

// All tests run serially to avoid race conditions when AGE creates labels
#[serial]
mod database_tests {
    use super::*;

    #[tokio::test]
    async fn test_connect() {
        let client = create_client().await;
        assert_eq!(client.graph_name(), TEST_GRAPH);
    }

    #[tokio::test]
    async fn test_execute_cypher_empty_result() {
        let client = create_client().await;
        cleanup(&client).await;

        let rows = client
            .query("MATCH (n:IntegrationTest) RETURN n")
            .fetch_all()
            .await
            .expect("Query failed");

        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn test_create_and_query_node() {
        let client = create_client().await;
        cleanup(&client).await;

        // Create a node
        client
            .query("CREATE (n:IntegrationTest {id: $id, name: $name})")
            .param("id", "test-node-1")
            .param("name", "Test Node")
            .run()
            .await
            .expect("Failed to create node");

        // Query it back
        let rows = client
            .query("MATCH (n:IntegrationTest {id: $id}) RETURN n")
            .param("id", "test-node-1")
            .fetch_all()
            .await
            .expect("Query failed");

        assert_eq!(rows.len(), 1);

        // Verify the result contains our data
        let result = rows[0].get_raw("n").expect("No result column");
        assert!(result.to_string().contains("test-node-1"));
        assert!(result.to_string().contains("Test Node"));

        cleanup(&client).await;
    }

    #[tokio::test]
    async fn test_transaction_commit() {
        let client = create_client().await;
        cleanup(&client).await;

        // Create node in transaction
        let txn = client.begin().await.expect("Failed to begin transaction");

        txn.query("CREATE (n:IntegrationTest {id: $id, name: $name})")
            .param("id", "txn-test-1")
            .param("name", "Transaction Test")
            .run()
            .await
            .expect("Failed to create node in transaction");

        txn.commit().await.expect("Failed to commit");

        // Verify node exists after commit
        let rows = client
            .query("MATCH (n:IntegrationTest {id: $id}) RETURN n")
            .param("id", "txn-test-1")
            .fetch_all()
            .await
            .expect("Query failed");

        assert_eq!(rows.len(), 1);

        cleanup(&client).await;
    }

    #[tokio::test]
    async fn test_transaction_rollback() {
        let client = create_client().await;
        cleanup(&client).await;

        // Create node in transaction
        let txn = client.begin().await.expect("Failed to begin transaction");

        txn.query("CREATE (n:IntegrationTest {id: $id, name: $name})")
            .param("id", "rollback-test-1")
            .param("name", "Rollback Test")
            .run()
            .await
            .expect("Failed to create node in transaction");

        txn.rollback().await.expect("Failed to rollback");

        // Verify node does NOT exist after rollback
        let rows = client
            .query("MATCH (n:IntegrationTest {id: $id}) RETURN n")
            .param("id", "rollback-test-1")
            .fetch_all()
            .await
            .expect("Query failed");

        assert!(rows.is_empty(), "Node should not exist after rollback");
    }

    #[tokio::test]
    async fn test_graph_wrapper() {
        let client = create_client().await;
        cleanup(&client).await;

        let graph = Graph::new(client);

        // Test direct query
        graph
            .query("CREATE (n:IntegrationTest {id: $id})")
            .param("id", "graph-test-1")
            .run()
            .await
            .expect("Failed to create node");

        let rows = graph
            .query("MATCH (n:IntegrationTest {id: $id}) RETURN n")
            .param("id", "graph-test-1")
            .fetch_all()
            .await
            .expect("Query failed");

        assert_eq!(rows.len(), 1);

        // Cleanup via graph
        graph
            .query("MATCH (n:IntegrationTest) DETACH DELETE n")
            .run()
            .await
            .expect("Cleanup failed");
    }

    #[tokio::test]
    async fn test_graph_transaction() {
        let client = create_client().await;
        cleanup(&client).await;

        let graph = Graph::new(client);

        // Use transaction closure
        let result = graph
            .transaction(|txn| async move {
                txn.query("CREATE (n:IntegrationTest {id: $id})")
                    .param("id", "graph-txn-1")
                    .run()
                    .await?;

                txn.query("CREATE (n:IntegrationTest {id: $id})")
                    .param("id", "graph-txn-2")
                    .run()
                    .await?;

                let rows = txn
                    .query("MATCH (n:IntegrationTest) RETURN n")
                    .fetch_all()
                    .await?;

                txn.commit().await?;
                Ok(rows.len())
            })
            .await
            .expect("Transaction failed");

        assert_eq!(result, 2);

        // Cleanup
        graph
            .query("MATCH (n:IntegrationTest) DETACH DELETE n")
            .run()
            .await
            .expect("Cleanup failed");
    }

    #[tokio::test]
    async fn test_sql_executor() {
        let client = create_client().await;

        let txn = client.begin().await.expect("Failed to begin transaction");

        // Execute raw SQL (not Cypher)
        txn.execute_sql("SELECT 1 as test")
            .await
            .expect("SQL execution failed");

        // Query SQL
        let rows = txn
            .query_sql("SELECT 'hello' as greeting, 42 as answer")
            .await
            .expect("SQL query failed");

        let rows: Vec<_> = rows.try_collect().await.expect("Failed to collect rows");

        assert_eq!(rows.len(), 1);
        let greeting: String = rows[0].get("greeting").expect("No greeting");
        assert_eq!(greeting, "hello");

        txn.rollback().await.expect("Failed to rollback");
    }

    #[tokio::test]
    async fn test_streaming_large_result() {
        let client = create_client().await;
        cleanup(&client).await;

        // Create multiple nodes
        for i in 0..10 {
            client
                .query("CREATE (n:IntegrationTest {id: $id, index: $idx})")
                .param("id", format!("stream-test-{}", i))
                .param("idx", i)
                .run()
                .await
                .expect("Failed to create node");
        }

        // Stream results
        let stream = client
            .query("MATCH (n:IntegrationTest) RETURN n")
            .execute()
            .await
            .expect("Query failed");

        let mut count = 0;
        futures::pin_mut!(stream);
        while let Some(result) = stream.next().await {
            result.expect("Failed to get row");
            count += 1;
        }

        assert_eq!(count, 10);

        cleanup(&client).await;
    }
}

// -----------------------------------------------------------------------------
// Injection Prevention Tests
//
// These tests prove that malicious input in parameters cannot escape
// the parameter context and execute as Cypher or SQL code.
// -----------------------------------------------------------------------------

#[serial]
mod injection_tests {
    use super::*;

    #[tokio::test]
    async fn test_cypher_injection_in_string_param() {
        let client = create_client().await;
        cleanup(&client).await;

        // Attempt Cypher injection via string parameter
        // If vulnerable, this would create an extra node or cause syntax error
        let malicious_id = "test' OR 1=1 --";

        client
            .query("CREATE (n:IntegrationTest {id: $id})")
            .param("id", malicious_id)
            .run()
            .await
            .expect("Should handle malicious input safely");

        // Verify only one node was created with the literal malicious string as ID
        let rows = client
            .query("MATCH (n:IntegrationTest) RETURN n.id as id")
            .fetch_all()
            .await
            .expect("Query failed");

        assert_eq!(rows.len(), 1, "Should create exactly one node");

        let stored_id: String = rows[0].get("id").expect("No id");
        assert_eq!(
            stored_id, malicious_id,
            "Malicious string should be stored literally, not executed"
        );

        cleanup(&client).await;
    }

    #[tokio::test]
    async fn test_cypher_injection_delete_attempt() {
        let client = create_client().await;
        cleanup(&client).await;

        // Create a legitimate node first
        client
            .query("CREATE (n:IntegrationTest {id: $id, secret: $secret})")
            .param("id", "legitimate")
            .param("secret", "confidential")
            .run()
            .await
            .expect("Failed to create node");

        // Attempt injection that would delete the node if vulnerable
        let malicious_id = "x' DETACH DELETE n WITH n MATCH (m) WHERE m.id = '";

        // This should NOT delete the legitimate node
        let rows = client
            .query("MATCH (n:IntegrationTest {id: $id}) RETURN n")
            .param("id", malicious_id)
            .fetch_all()
            .await
            .expect("Query should execute safely");

        // Should return 0 rows (no match), not cause deletion
        assert_eq!(rows.len(), 0, "Should not find malicious id");

        // Verify legitimate node still exists
        let rows = client
            .query("MATCH (n:IntegrationTest {id: $id}) RETURN n.secret as secret")
            .param("id", "legitimate")
            .fetch_all()
            .await
            .expect("Query failed");

        assert_eq!(rows.len(), 1, "Legitimate node should still exist");
        let secret: String = rows[0].get("secret").expect("No secret");
        assert_eq!(secret, "confidential", "Data should be intact");

        cleanup(&client).await;
    }

    #[tokio::test]
    async fn test_cypher_injection_json_escape() {
        let client = create_client().await;
        cleanup(&client).await;

        // Attempt injection via JSON special characters
        let malicious_name = r#"{"$id": "injected"}, extra: "payload"}"#;

        client
            .query("CREATE (n:IntegrationTest {id: $id, name: $name})")
            .param("id", "json-test")
            .param("name", malicious_name)
            .run()
            .await
            .expect("Should handle JSON-like strings safely");

        let rows = client
            .query("MATCH (n:IntegrationTest {id: $id}) RETURN n.name as name")
            .param("id", "json-test")
            .fetch_all()
            .await
            .expect("Query failed");

        assert_eq!(rows.len(), 1);
        let stored_name: String = rows[0].get("name").expect("No name");
        assert_eq!(
            stored_name, malicious_name,
            "JSON-like string should be stored literally"
        );

        cleanup(&client).await;
    }

    #[tokio::test]
    async fn test_cypher_injection_null_bytes() {
        let client = create_client().await;
        cleanup(&client).await;

        // Attempt injection with null bytes (common attack vector)
        let malicious_id = "test\x00' OR '1'='1";

        // PostgreSQL typically rejects null bytes, which is fine -
        // the important thing is it doesn't execute the injection
        let result = client
            .query("CREATE (n:IntegrationTest {id: $id})")
            .param("id", malicious_id)
            .run()
            .await;

        // Either succeeds (storing literally) or fails (rejecting null byte)
        // Both are safe outcomes - injection is not possible
        match result {
            Ok(_) => {
                // Verify stored literally
                let rows = client
                    .query("MATCH (n:IntegrationTest) RETURN n.id as id")
                    .fetch_all()
                    .await
                    .expect("Query failed");
                assert_eq!(rows.len(), 1);
            }
            Err(_) => {
                // Null byte rejected - also safe
            }
        }

        cleanup(&client).await;
    }
}

// -----------------------------------------------------------------------------
// Migration Tests
// -----------------------------------------------------------------------------

#[serial]
mod migration_tests {
    use super::*;

    #[tokio::test]
    async fn test_run_migrations() {
        let client = create_client().await;

        // Run migrations
        let result = run_migrations(&client)
            .await
            .expect("Migrations should succeed");

        println!("Migration result: {:?}", result);

        // On a fresh db (after db-reset), we should apply all 5 migrations
        // On subsequent runs, we may apply 0 if already at latest version
        assert!(
            result.current_version >= 5,
            "Should be at version 5 or higher"
        );

        // Verify schema_version table exists and has correct version
        let txn = client.begin().await.expect("Failed to begin transaction");
        let rows: Vec<_> = txn
            .query_sql("SELECT version, applied_migrations FROM schema_version WHERE id = 1")
            .await
            .expect("Should query schema_version")
            .try_collect()
            .await
            .expect("Should collect rows");
        txn.commit().await.expect("Failed to commit");

        assert_eq!(rows.len(), 1, "Should have one schema_version row");
        let version: i64 = rows[0].get("version").expect("Should have version");
        assert!(version >= 5, "Version should be >= 5");

        // Verify the AGE graph exists
        let txn = client.begin().await.expect("Failed to begin transaction");
        let rows: Vec<_> = txn
            .query_sql("SELECT name FROM ag_catalog.ag_graph WHERE name = 'knowledge_graph'")
            .await
            .expect("Should query ag_graph")
            .try_collect()
            .await
            .expect("Should collect rows");
        txn.commit().await.expect("Failed to commit");

        assert_eq!(rows.len(), 1, "knowledge_graph should exist");

        // Verify seed data: Scopes exist
        let rows = client
            .query("MATCH (s:Scope) RETURN s.name as name ORDER BY s.depth")
            .fetch_all()
            .await
            .expect("Should query scopes");

        let scope_names: Vec<String> = rows
            .iter()
            .map(|r| r.get::<String>("name").unwrap_or_default())
            .collect();

        assert!(
            scope_names.contains(&"Domain".to_string()),
            "Should have Domain scope"
        );
        assert!(
            scope_names.contains(&"Feature".to_string()),
            "Should have Feature scope"
        );
        assert!(
            scope_names.contains(&"Unit".to_string()),
            "Should have Unit scope"
        );

        // Verify seed data: Categories exist
        let rows = client
            .query("MATCH (c:Category) RETURN count(c) as cnt")
            .fetch_all()
            .await
            .expect("Should query categories");

        let count: i64 = rows[0].get("cnt").unwrap_or(0);
        assert!(count >= 17, "Should have at least 17 default categories");

        // Verify trigger functions exist
        let txn = client.begin().await.expect("Failed to begin transaction");
        let rows: Vec<_> = txn
            .query_sql(
                "SELECT proname FROM pg_proc WHERE proname IN (
                    'prevent_delete_with_children',
                    'cascade_delete_entity_references',
                    'validate_belongs_to_scope',
                    'attach_graph_triggers'
                )",
            )
            .await
            .expect("Should query functions")
            .try_collect()
            .await
            .expect("Should collect rows");
        txn.commit().await.expect("Failed to commit");

        assert!(
            rows.len() >= 4,
            "Should have at least 4 trigger functions, found {}",
            rows.len()
        );

        // Verify embeddings table exists
        let txn = client.begin().await.expect("Failed to begin transaction");
        let result = txn.execute_sql("SELECT 1 FROM embeddings LIMIT 1").await;
        txn.rollback().await.expect("Failed to rollback");

        assert!(result.is_ok(), "embeddings table should exist");

        println!("All migration tests passed!");
    }

    #[tokio::test]
    async fn test_migrations_are_idempotent() {
        let client = create_client().await;

        // Run migrations twice
        let result1 = run_migrations(&client)
            .await
            .expect("First migration run should succeed");

        let result2 = run_migrations(&client)
            .await
            .expect("Second migration run should succeed");

        // Second run should apply 0 migrations (already up to date)
        assert_eq!(
            result2.applied_migrations.len(),
            0,
            "Second run should not apply any migrations"
        );

        // Versions should match
        assert_eq!(
            result1.current_version, result2.current_version,
            "Versions should match"
        );
    }
}
