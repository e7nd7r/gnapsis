//! PostgreSQL + Apache AGE backend implementation.
//!
//! This module provides a graph database backend using PostgreSQL with the
//! Apache AGE extension for Cypher query support.
//!
//! # Example
//!
//! ```ignore
//! use gnapsis::graph::backends::postgres::PostgresClient;
//! use gnapsis::graph::{Graph, QueryExt};
//!
//! let client = PostgresClient::connect("postgresql://localhost/mydb", "my_graph").await?;
//! let graph = Graph::new(client);
//!
//! let rows = graph.query("MATCH (n:Entity) RETURN n")
//!     .fetch_all()
//!     .await?;
//! ```

use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::BytesMut;
use deadpool_postgres::{Manager, ManagerConfig, Object, Pool, RecyclingMethod};
use futures::TryStreamExt;
use serde_json::Value as JsonValue;
use tokio_postgres::types::{to_sql_checked, IsNull, ToSql, Type};
use tokio_postgres::NoTls;

use crate::error::AppError;
use crate::graph::row::{Params, Row, RowStream};

// ----------------------------------------------------------------------------
// Agtype wrapper for AGE parameter binding
// ----------------------------------------------------------------------------

/// Wrapper for AGE agtype parameter values.
///
/// AGE's `cypher()` function requires parameters to be of type `agtype`,
/// not TEXT. This wrapper implements `ToSql` to properly serialize JSON
/// parameters in agtype's binary format.
///
/// # Binary Format
///
/// Agtype uses a simple binary format:
/// - 1 byte: version (currently 1)
/// - N bytes: JSON text
///
/// # Security
///
/// Parameters are sent as typed binary data via PostgreSQL's extended query
/// protocol. They are **never** interpolated into the Cypher query string,
/// making injection attacks structurally impossible.
#[derive(Debug, Clone)]
struct Agtype(String);

impl Agtype {
    /// Creates a new Agtype from a JSON string.
    fn new(json: String) -> Self {
        Self(json)
    }
}

impl ToSql for Agtype {
    fn to_sql(
        &self,
        _ty: &Type,
        out: &mut BytesMut,
    ) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
        // Agtype binary format: version byte (1) followed by JSON text
        out.extend_from_slice(&[1]); // Version byte
        out.extend_from_slice(self.0.as_bytes());
        Ok(IsNull::No)
    }

    fn accepts(ty: &Type) -> bool {
        // Accept agtype by name since OID varies per database installation
        ty.name() == "agtype"
    }

    to_sql_checked!();
}
use crate::graph::traits::{CypherExecutor, GraphClient, SqlExecutor, Transaction};

/// PostgreSQL + Apache AGE graph client.
///
/// Provides connection pooling via deadpool-postgres and executes Cypher
/// queries through the AGE extension.
pub struct PostgresClient {
    pool: Pool,
    graph_name: Arc<str>,
}

impl PostgresClient {
    /// Creates a new PostgreSQL client with connection pooling.
    ///
    /// # Arguments
    ///
    /// * `connection_string` - PostgreSQL connection string (e.g., "postgresql://user:pass@host/db")
    /// * `graph_name` - Name of the AGE graph to use for Cypher queries
    ///
    /// # Example
    ///
    /// ```ignore
    /// let client = PostgresClient::connect(
    ///     "postgresql://postgres:password@localhost:5432/myproject",
    ///     "knowledge_graph"
    /// ).await?;
    /// ```
    pub async fn connect(connection_string: &str, graph_name: &str) -> Result<Self, AppError> {
        let pg_config: tokio_postgres::Config = connection_string.parse().map_err(|e| {
            AppError::Internal(format!("Invalid PostgreSQL connection string: {}", e))
        })?;

        let mgr_config = ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        };
        let mgr = Manager::from_config(pg_config, NoTls, mgr_config);
        let pool = Pool::builder(mgr)
            .max_size(16)
            .build()
            .map_err(|e| AppError::Internal(format!("Failed to create connection pool: {}", e)))?;

        Ok(Self {
            pool,
            graph_name: Arc::from(graph_name),
        })
    }

    /// Gets a connection from the pool with AGE session setup.
    async fn get_connection(&self) -> Result<Object, AppError> {
        let conn = self.pool.get().await.map_err(|e| {
            AppError::Internal(format!("Failed to get connection from pool: {}", e))
        })?;

        // Initialize AGE session on this connection
        conn.batch_execute("LOAD 'age'; SET search_path = ag_catalog, public;")
            .await
            .map_err(|e| AppError::Internal(format!("Failed to initialize AGE session: {}", e)))?;

        Ok(conn)
    }

    /// Returns a reference to the graph name.
    pub fn graph_name(&self) -> &str {
        &self.graph_name
    }
}

#[async_trait]
impl CypherExecutor for PostgresClient {
    async fn execute_cypher(
        &self,
        cypher: &str,
        params: Params,
    ) -> Result<RowStream<'_>, AppError> {
        let conn = self.get_connection().await?;
        execute_pg_cypher_owned(conn, self.graph_name.clone(), cypher.to_string(), params)
    }

    async fn run_cypher(&self, cypher: &str, params: Params) -> Result<(), AppError> {
        use futures::StreamExt;
        let mut stream = self.execute_cypher(cypher, params).await?;
        // Drain the stream to ensure the query completes
        while let Some(result) = stream.next().await {
            result?;
        }
        Ok(())
    }
}

#[async_trait]
impl GraphClient for PostgresClient {
    type Tx<'a> = PostgresTransaction;

    async fn begin(&self) -> Result<Self::Tx<'_>, AppError> {
        let conn = self.get_connection().await?;

        // Start transaction manually
        conn.batch_execute("BEGIN")
            .await
            .map_err(|e| AppError::Internal(format!("Failed to begin transaction: {}", e)))?;

        Ok(PostgresTransaction {
            conn,
            graph_name: self.graph_name.clone(),
            finished: false,
        })
    }
}

/// PostgreSQL transaction with Cypher and SQL support.
///
/// Wraps a pooled connection with an active transaction. The transaction
/// must be explicitly committed or rolled back; dropping without committing
/// will log a warning (the connection returns to the pool in an undefined state).
pub struct PostgresTransaction {
    conn: Object,
    graph_name: Arc<str>,
    finished: bool,
}

#[async_trait]
impl CypherExecutor for PostgresTransaction {
    async fn execute_cypher(
        &self,
        cypher: &str,
        params: Params,
    ) -> Result<RowStream<'_>, AppError> {
        execute_pg_cypher(&self.conn, &self.graph_name, cypher, params).await
    }

    async fn run_cypher(&self, cypher: &str, params: Params) -> Result<(), AppError> {
        use futures::StreamExt;
        let mut stream = self.execute_cypher(cypher, params).await?;
        // Drain the stream to ensure the query completes
        while let Some(result) = stream.next().await {
            result?;
        }
        Ok(())
    }
}

#[async_trait]
impl SqlExecutor for PostgresTransaction {
    async fn execute_sql(&self, sql: &str) -> Result<(), AppError> {
        self.conn
            .batch_execute(sql)
            .await
            .map_err(|e| AppError::Internal(format!("SQL execution failed: {}", e)))?;
        Ok(())
    }

    async fn query_sql(&self, sql: &str) -> Result<RowStream<'_>, AppError> {
        let stream = self
            .conn
            .query_raw::<_, &str, _>(sql, std::iter::empty())
            .await
            .map_err(|e| AppError::Internal(format!("SQL query failed: {}", e)))?;

        Ok(Box::pin(stream.map_ok(|row| parse_pg_row(&row)).map_err(
            |e| AppError::Internal(format!("Failed to fetch row: {}", e)),
        )))
    }
}

#[async_trait]
impl Transaction for PostgresTransaction {
    async fn commit(mut self) -> Result<(), AppError> {
        self.finished = true;
        self.conn
            .batch_execute("COMMIT")
            .await
            .map_err(|e| AppError::Internal(format!("Failed to commit transaction: {}", e)))?;
        Ok(())
    }

    async fn rollback(mut self) -> Result<(), AppError> {
        self.finished = true;
        self.conn
            .batch_execute("ROLLBACK")
            .await
            .map_err(|e| AppError::Internal(format!("Failed to rollback transaction: {}", e)))?;
        Ok(())
    }
}

impl Drop for PostgresTransaction {
    fn drop(&mut self) {
        if !self.finished {
            tracing::warn!(
                "PostgresTransaction dropped without commit or rollback - connection state undefined"
            );
        }
    }
}

/// Executes a Cypher query via AGE on a borrowed connection.
///
/// This is used by `PostgresTransaction` where we borrow the connection.
async fn execute_pg_cypher<'a>(
    conn: &'a Object,
    graph_name: &str,
    cypher: &str,
    params: Params,
) -> Result<RowStream<'a>, AppError> {
    let (sql, agtype_param) = build_age_query(graph_name, cypher, &params)?;

    // Use query_raw for streaming results
    // Agtype wrapper ensures proper binary serialization for AGE parameters
    let stream = match &agtype_param {
        None => {
            conn.query_raw::<_, &Agtype, _>(&sql, std::iter::empty())
                .await
        }
        Some(param) => conn.query_raw(&sql, std::iter::once(param)).await,
    };

    let stream = stream.map_err(|e| AppError::Query {
        message: format!("Cypher query failed: {}", e),
        query: cypher.to_string(),
    })?;

    Ok(Box::pin(stream.map_ok(|row| parse_pg_row(&row)).map_err(
        |e| AppError::Internal(format!("Failed to fetch row: {}", e)),
    )))
}

/// Executes a Cypher query via AGE on an owned connection.
///
/// This is used by `PostgresClient` for auto-commit queries where
/// we own the connection for the duration of the stream.
///
/// Uses `async_stream` to create a generator that captures the connection,
/// keeping it alive naturally for the stream's lifetime.
fn execute_pg_cypher_owned(
    conn: Object,
    graph_name: Arc<str>,
    cypher: String,
    params: Params,
) -> Result<RowStream<'static>, AppError> {
    use async_stream::try_stream;

    let (sql, agtype_param) = build_age_query(&graph_name, &cypher, &params)?;

    Ok(Box::pin(try_stream! {
        // conn is captured by the generator and kept alive
        let stream = match &agtype_param {
            None => conn.query_raw::<_, &Agtype, _>(&sql, std::iter::empty()).await,
            Some(param) => conn.query_raw(&sql, std::iter::once(param)).await,
        };

        let stream = stream.map_err(|e| AppError::Query {
            message: format!("Cypher query failed: {}", e),
            query: cypher.clone(),
        })?;

        futures::pin_mut!(stream);
        while let Some(pg_row) = stream.try_next().await.map_err(|e| {
            AppError::Internal(format!("Failed to fetch row: {}", e))
        })? {
            yield parse_pg_row(&pg_row);
        }
    }))
}

/// Builds the AGE SQL query for a Cypher statement.
///
/// Apache AGE requires SQL column definitions to match the Cypher RETURN clause.
/// This function parses the Cypher query to extract column names and generates
/// appropriate SQL column definitions.
///
/// # Example
///
/// For `RETURN a, r AS rel, b`, generates:
/// `SELECT * FROM cypher(...) as (a agtype, rel agtype, b agtype)`
///
/// Returns the SQL string and an optional Agtype parameter.
/// When params is empty, no parameter is needed.
fn build_age_query(
    graph_name: &str,
    cypher: &str,
    params: &Params,
) -> Result<(String, Option<Agtype>), AppError> {
    use crate::graph::cypher::{extract_return_columns, ParseError};

    // Extract column names from RETURN clause
    let columns = extract_return_columns(cypher).map_err(|e| match e {
        ParseError::ReturnStarNotSupported => AppError::Internal(
            "RETURN * is not supported - please specify columns explicitly".into(),
        ),
        ParseError::NoReturnClause => AppError::Internal("Query must have a RETURN clause".into()),
        ParseError::InvalidSyntax(msg) => {
            AppError::Internal(format!("Cypher syntax error: {}", msg))
        }
    })?;

    // Generate column definitions for SQL
    // Each column name becomes: "column_name agtype"
    // We need to quote column names that contain special characters
    let column_defs: Vec<String> = columns
        .iter()
        .map(|name| {
            // Quote column names that contain special characters
            if name.chars().all(|c| c.is_alphanumeric() || c == '_')
                && !name.starts_with(|c: char| c.is_numeric())
            {
                format!("{} agtype", name)
            } else {
                // Use quoted identifier for names with special characters
                format!("\"{}\" agtype", name.replace('"', "\"\""))
            }
        })
        .collect();

    let columns_sql = column_defs.join(", ");

    if params.is_empty() {
        let sql = format!(
            "SELECT * FROM cypher('{}', $$ {} $$) as ({})",
            graph_name, cypher, columns_sql
        );
        Ok((sql, None))
    } else {
        let sql = format!(
            "SELECT * FROM cypher('{}', $$ {} $$, $1) as ({})",
            graph_name, cypher, columns_sql
        );
        let params_json = serde_json::to_string(params)
            .map_err(|e| AppError::Internal(format!("Failed to serialize parameters: {}", e)))?;
        Ok((sql, Some(Agtype::new(params_json))))
    }
}

/// Parses a PostgreSQL row into our generic Row type.
///
/// AGE returns results as `agtype` in a column named "result".
/// We parse the agtype and convert to JSON.
fn parse_pg_row(pg_row: &tokio_postgres::Row) -> Row {
    let mut data = HashMap::new();

    for (idx, column) in pg_row.columns().iter().enumerate() {
        let name = column.name().to_string();
        let col_type = column.type_();

        let value = if col_type.name() == "agtype" {
            // AGE agtype: use our custom FromSql implementation
            pg_row
                .try_get::<_, AgtypeValue>(idx)
                .ok()
                .map(|v| v.0)
                .unwrap_or(JsonValue::Null)
        } else {
            // Standard types: try JSON first, then string fallback
            pg_row
                .try_get::<_, JsonValue>(idx)
                .ok()
                .or_else(|| pg_row.try_get::<_, String>(idx).ok().map(JsonValue::String))
                .unwrap_or(JsonValue::Null)
        };

        data.insert(name, value);
    }

    Row::new(data)
}

/// Wrapper for reading agtype values from PostgreSQL.
///
/// Implements FromSql to properly deserialize AGE's agtype binary format.
#[derive(Debug)]
struct AgtypeValue(JsonValue);

impl<'a> tokio_postgres::types::FromSql<'a> for AgtypeValue {
    fn from_sql(_ty: &Type, raw: &'a [u8]) -> Result<Self, Box<dyn Error + Sync + Send>> {
        // Agtype binary format: 1 byte version + JSON text
        if raw.is_empty() {
            return Ok(AgtypeValue(JsonValue::Null));
        }

        // Skip version byte (first byte)
        let json_bytes = if raw[0] == 1 { &raw[1..] } else { raw };
        let json_str = std::str::from_utf8(json_bytes)?;

        // Remove type suffixes (::vertex, ::edge, ::path)
        let clean_json = json_str
            .trim_end_matches("::vertex")
            .trim_end_matches("::edge")
            .trim_end_matches("::path");

        let value = serde_json::from_str(clean_json)?;
        Ok(AgtypeValue(value))
    }

    fn accepts(ty: &Type) -> bool {
        ty.name() == "agtype"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_age_query_no_params() {
        let params = Params::new();
        let (sql, agtype_param) =
            build_age_query("test_graph", "MATCH (n) RETURN n", &params).unwrap();

        // Column name is extracted from RETURN clause
        assert_eq!(
            sql,
            "SELECT * FROM cypher('test_graph', $$ MATCH (n) RETURN n $$) as (n agtype)"
        );
        assert!(agtype_param.is_none());
    }

    #[test]
    fn test_build_age_query_with_params() {
        let mut params = Params::new();
        params.insert("id".to_string(), JsonValue::String("test-123".to_string()));

        let (sql, agtype_param) =
            build_age_query("test_graph", "MATCH (n) WHERE n.id = $id RETURN n", &params).unwrap();

        // Column name is extracted from RETURN clause
        assert_eq!(
            sql,
            "SELECT * FROM cypher('test_graph', $$ MATCH (n) WHERE n.id = $id RETURN n $$, $1) as (n agtype)"
        );
        let param = agtype_param.expect("Should have agtype param");
        assert!(param.0.contains("test-123"));
    }

    #[test]
    fn test_build_age_query_multiple_columns() {
        let params = Params::new();
        let (sql, _) = build_age_query(
            "test_graph",
            "MATCH (a)-[r]->(b) RETURN a, r AS rel, b",
            &params,
        )
        .unwrap();

        // Multiple columns extracted from RETURN clause, with alias
        assert_eq!(
            sql,
            "SELECT * FROM cypher('test_graph', $$ MATCH (a)-[r]->(b) RETURN a, r AS rel, b $$) as (a agtype, rel agtype, b agtype)"
        );
    }

    #[test]
    fn test_build_age_query_property_access() {
        let params = Params::new();
        let (sql, _) = build_age_query(
            "test_graph",
            "MATCH (n) RETURN n.name, n.age AS age",
            &params,
        )
        .unwrap();

        // Property access uses quoted identifier, alias uses plain name
        assert_eq!(
            sql,
            "SELECT * FROM cypher('test_graph', $$ MATCH (n) RETURN n.name, n.age AS age $$) as (\"n.name\" agtype, age agtype)"
        );
    }
}
