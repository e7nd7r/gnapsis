//! Row and streaming types for query results.

use crate::error::AppError;
use futures::Stream;
use serde::de::DeserializeOwned;
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
}
