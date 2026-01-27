//! Row and streaming types for query results.

use crate::error::AppError;
use futures::Stream;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::pin::Pin;

/// Parameters for Cypher queries.
///
/// A map of parameter names to JSON values that can be passed to queries.
pub type Params = HashMap<String, JsonValue>;

/// A stream of rows from a query result.
///
/// Uses `futures::Stream` for memory-efficient iteration over large result sets.
/// Rows are fetched on-demand, not loaded all at once.
pub type RowStream<'a> = Pin<Box<dyn Stream<Item = Result<Row, AppError>> + Send + 'a>>;

// ----------------------------------------------------------------------------
// Graph Types: Node, Relation, Path
// ----------------------------------------------------------------------------

/// A graph node (vertex) from a Cypher query result.
///
/// Represents a node with its internal ID, label, and properties.
/// Compatible with both Neo4j and Apache AGE backends.
///
/// # Example
///
/// ```ignore
/// let node: Node = row.get("n")?;
/// let id: String = node.get("id")?;
/// let name: String = node.get("name")?;
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    /// Internal graph database ID (not the application-level ID).
    #[serde(alias = "id")]
    pub graph_id: i64,
    /// Node label (e.g., "Entity", "Reference").
    pub label: String,
    /// Node properties as a JSON object.
    pub properties: JsonValue,
}

impl Node {
    /// Gets a property value by name, deserializing to the requested type.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let id: String = node.get("id")?;
    /// let count: i64 = node.get("count")?;
    /// ```
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Result<T, AppError> {
        self.properties
            .get(key)
            .ok_or_else(|| AppError::Internal(format!("property not found: {}", key)))
            .and_then(|v| {
                serde_json::from_value(v.clone()).map_err(|e| {
                    AppError::Internal(format!("failed to deserialize '{}': {}", key, e))
                })
            })
    }

    /// Gets a property value, returning `None` if it doesn't exist.
    pub fn get_opt<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, AppError> {
        match self.properties.get(key) {
            Some(v) if v.is_null() => Ok(None),
            Some(v) => serde_json::from_value(v.clone())
                .map(Some)
                .map_err(|e| AppError::Internal(format!("failed to deserialize '{}': {}", key, e))),
            None => Ok(None),
        }
    }

    /// Returns the raw JSON value for a property.
    pub fn get_raw(&self, key: &str) -> Option<&JsonValue> {
        self.properties.get(key)
    }

    /// Returns all property names.
    pub fn property_names(&self) -> Vec<&str> {
        self.properties
            .as_object()
            .map(|obj| obj.keys().map(|s| s.as_str()).collect())
            .unwrap_or_default()
    }
}

/// A graph relationship (edge) from a Cypher query result.
///
/// Represents a directed relationship between two nodes.
///
/// # Example
///
/// ```ignore
/// let rel: Relation = row.get("r")?;
/// println!("Type: {}", rel.rel_type);
/// let weight: f64 = rel.get("weight")?;
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    /// Internal graph database ID.
    #[serde(alias = "id")]
    pub graph_id: i64,
    /// Relationship type (e.g., "BELONGS_TO", "REFERENCES").
    #[serde(alias = "label")]
    pub rel_type: String,
    /// Start node's internal ID.
    pub start_id: i64,
    /// End node's internal ID.
    pub end_id: i64,
    /// Relationship properties.
    pub properties: JsonValue,
}

impl Relation {
    /// Gets a property value by name.
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Result<T, AppError> {
        self.properties
            .get(key)
            .ok_or_else(|| AppError::Internal(format!("property not found: {}", key)))
            .and_then(|v| {
                serde_json::from_value(v.clone()).map_err(|e| {
                    AppError::Internal(format!("failed to deserialize '{}': {}", key, e))
                })
            })
    }

    /// Gets a property value, returning `None` if it doesn't exist.
    pub fn get_opt<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, AppError> {
        match self.properties.get(key) {
            Some(v) if v.is_null() => Ok(None),
            Some(v) => serde_json::from_value(v.clone())
                .map(Some)
                .map_err(|e| AppError::Internal(format!("failed to deserialize '{}': {}", key, e))),
            None => Ok(None),
        }
    }
}

/// A path through the graph (sequence of nodes and relationships).
///
/// Paths are returned by queries like `MATCH p = (a)-[*]->(b) RETURN p`.
///
/// # Example
///
/// ```ignore
/// let path: Path = row.get("p")?;
/// for node in path.nodes() {
///     println!("Node: {}", node.label);
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Path {
    /// Alternating sequence of nodes and relationships.
    /// Always starts and ends with a node: [Node, Rel, Node, Rel, Node, ...]
    elements: Vec<PathElement>,
}

/// An element in a path (either a node or relationship).
#[derive(Debug, Clone)]
pub enum PathElement {
    Node(Node),
    Relation(Relation),
}

impl Path {
    /// Creates a path from a sequence of elements.
    pub fn new(elements: Vec<PathElement>) -> Self {
        Self { elements }
    }

    /// Returns all nodes in the path.
    pub fn nodes(&self) -> impl Iterator<Item = &Node> {
        self.elements.iter().filter_map(|e| match e {
            PathElement::Node(n) => Some(n),
            _ => None,
        })
    }

    /// Returns all relationships in the path.
    pub fn relations(&self) -> impl Iterator<Item = &Relation> {
        self.elements.iter().filter_map(|e| match e {
            PathElement::Relation(r) => Some(r),
            _ => None,
        })
    }

    /// Returns the number of relationships in the path (path length).
    pub fn len(&self) -> usize {
        self.relations().count()
    }

    /// Returns true if the path has no relationships.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the start node of the path.
    pub fn start(&self) -> Option<&Node> {
        self.nodes().next()
    }

    /// Returns the end node of the path.
    pub fn end(&self) -> Option<&Node> {
        self.nodes().last()
    }
}

/// A single row from a query result.
///
/// Contains column values as JSON, with typed extraction via [`Row::get`].
#[derive(Debug, Clone)]
pub struct Row {
    data: HashMap<String, JsonValue>,
}

impl Row {
    /// Creates a new row from a map of column names to values.
    pub fn new(data: HashMap<String, JsonValue>) -> Self {
        Self { data }
    }

    /// Gets a value from the row by column name, deserializing to the requested type.
    ///
    /// # Errors
    ///
    /// Returns an error if the key is not found or if deserialization fails.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let id: String = row.get("id")?;
    /// let count: i64 = row.get("count")?;
    /// ```
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Result<T, AppError> {
        self.data
            .get(key)
            .ok_or_else(|| AppError::Internal(format!("column not found: {}", key)))
            .and_then(|v| {
                serde_json::from_value(v.clone()).map_err(|e| {
                    AppError::Internal(format!("failed to deserialize '{}': {}", key, e))
                })
            })
    }

    /// Gets a value from the row, returning `None` if the key doesn't exist.
    ///
    /// Still returns an error if the key exists but deserialization fails.
    pub fn get_opt<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, AppError> {
        match self.data.get(key) {
            Some(v) if v.is_null() => Ok(None),
            Some(v) => serde_json::from_value(v.clone())
                .map(Some)
                .map_err(|e| AppError::Internal(format!("failed to deserialize '{}': {}", key, e))),
            None => Ok(None),
        }
    }

    /// Returns the raw JSON value for a column, if it exists.
    pub fn get_raw(&self, key: &str) -> Option<&JsonValue> {
        self.data.get(key)
    }

    /// Returns all column names in this row.
    pub fn columns(&self) -> impl Iterator<Item = &str> {
        self.data.keys().map(|s| s.as_str())
    }

    /// Returns the number of columns in this row.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Returns true if the row has no columns.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Consumes the row and returns the underlying data map.
    pub fn into_inner(self) -> HashMap<String, JsonValue> {
        self.data
    }
}

impl From<HashMap<String, JsonValue>> for Row {
    fn from(data: HashMap<String, JsonValue>) -> Self {
        Self::new(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_row_get_string() {
        let mut data = HashMap::new();
        data.insert("name".to_string(), json!("test"));
        let row = Row::new(data);

        let name: String = row.get("name").unwrap();
        assert_eq!(name, "test");
    }

    #[test]
    fn test_row_get_number() {
        let mut data = HashMap::new();
        data.insert("count".to_string(), json!(42));
        let row = Row::new(data);

        let count: i64 = row.get("count").unwrap();
        assert_eq!(count, 42);
    }

    #[test]
    fn test_row_get_missing_key() {
        let row = Row::new(HashMap::new());
        let result: Result<String, _> = row.get("missing");
        assert!(result.is_err());
    }

    #[test]
    fn test_row_get_opt_present() {
        let mut data = HashMap::new();
        data.insert("name".to_string(), json!("test"));
        let row = Row::new(data);

        let name: Option<String> = row.get_opt("name").unwrap();
        assert_eq!(name, Some("test".to_string()));
    }

    #[test]
    fn test_row_get_opt_missing() {
        let row = Row::new(HashMap::new());
        let name: Option<String> = row.get_opt("missing").unwrap();
        assert_eq!(name, None);
    }

    #[test]
    fn test_row_get_opt_null() {
        let mut data = HashMap::new();
        data.insert("name".to_string(), JsonValue::Null);
        let row = Row::new(data);

        let name: Option<String> = row.get_opt("name").unwrap();
        assert_eq!(name, None);
    }

    #[test]
    fn test_row_columns() {
        let mut data = HashMap::new();
        data.insert("a".to_string(), json!(1));
        data.insert("b".to_string(), json!(2));
        let row = Row::new(data);

        let mut columns: Vec<_> = row.columns().collect();
        columns.sort();
        assert_eq!(columns, vec!["a", "b"]);
    }

    #[test]
    fn test_node_from_age_json() {
        // AGE returns vertices in this format (after stripping ::vertex suffix)
        let age_vertex = json!({
            "id": 844424930131969_i64,
            "label": "Entity",
            "properties": {
                "id": "entity-123",
                "name": "Test Entity",
                "count": 42
            }
        });

        let mut data = HashMap::new();
        data.insert("n".to_string(), age_vertex);
        let row = Row::new(data);

        // Deserialize to Node
        let node: Node = row.get("n").unwrap();
        assert_eq!(node.graph_id, 844424930131969);
        assert_eq!(node.label, "Entity");

        // Access properties
        let id: String = node.get("id").unwrap();
        assert_eq!(id, "entity-123");

        let name: String = node.get("name").unwrap();
        assert_eq!(name, "Test Entity");

        let count: i64 = node.get("count").unwrap();
        assert_eq!(count, 42);
    }

    #[test]
    fn test_relation_from_age_json() {
        // AGE returns edges in this format (after stripping ::edge suffix)
        let age_edge = json!({
            "id": 1125899906842625_i64,
            "label": "BELONGS_TO",
            "start_id": 844424930131969_i64,
            "end_id": 844424930131970_i64,
            "properties": {
                "since": "2024-01-01",
                "weight": 0.95
            }
        });

        let mut data = HashMap::new();
        data.insert("r".to_string(), age_edge);
        let row = Row::new(data);

        // Deserialize to Relation
        let rel: Relation = row.get("r").unwrap();
        assert_eq!(rel.graph_id, 1125899906842625);
        assert_eq!(rel.rel_type, "BELONGS_TO");
        assert_eq!(rel.start_id, 844424930131969);
        assert_eq!(rel.end_id, 844424930131970);

        // Access properties
        let since: String = rel.get("since").unwrap();
        assert_eq!(since, "2024-01-01");

        let weight: f64 = rel.get("weight").unwrap();
        assert!((weight - 0.95).abs() < 0.001);
    }

    #[test]
    fn test_mixed_row_with_node_and_scalar() {
        // Simulates: MATCH (n:Entity) RETURN n, n.name AS name
        let age_vertex = json!({
            "id": 844424930131969_i64,
            "label": "Entity",
            "properties": {"name": "Test"}
        });

        let mut data = HashMap::new();
        data.insert("n".to_string(), age_vertex);
        data.insert("name".to_string(), json!("Test"));
        let row = Row::new(data);

        // Get node
        let node: Node = row.get("n").unwrap();
        assert_eq!(node.label, "Entity");

        // Get scalar directly
        let name: String = row.get("name").unwrap();
        assert_eq!(name, "Test");
    }
}
