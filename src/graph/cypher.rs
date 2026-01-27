//! Cypher query parser for extracting RETURN clause column names.
//!
//! This module provides a parser based on the official openCypher grammar
//! to extract column names from Cypher queries, enabling automatic SQL
//! column generation for Apache AGE.
//!
//! # Example
//!
//! ```
//! use gnapsis::graph::extract_return_columns;
//!
//! let columns = extract_return_columns("MATCH (n) RETURN n.name AS name, n.age").unwrap();
//! assert_eq!(columns, vec!["name", "n.age"]);
//! ```

use pest::Parser;
use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "graph/cypher.pest"]
struct CypherParser;

/// Extracts column names from a Cypher query's RETURN clause.
///
/// Parses the complete Cypher query and extracts the column names from
/// the final RETURN clause. For aliased expressions (`expr AS alias`),
/// returns the alias. For unaliased expressions, returns the expression text.
///
/// # Arguments
///
/// * `query` - A Cypher query string
///
/// # Returns
///
/// A vector of column names for each return item.
///
/// # Example
///
/// ```
/// use gnapsis::graph::extract_return_columns;
///
/// // Aliased returns use the alias
/// let cols = extract_return_columns("MATCH (n) RETURN n.name AS name").unwrap();
/// assert_eq!(cols, vec!["name"]);
///
/// // Unaliased returns use the expression text
/// let cols = extract_return_columns("MATCH (n) RETURN n.name, n.age + 10").unwrap();
/// assert_eq!(cols, vec!["n.name", "n.age + 10"]);
///
/// // Mixed
/// let cols = extract_return_columns("RETURN a, r AS rel, b").unwrap();
/// assert_eq!(cols, vec!["a", "rel", "b"]);
/// ```
pub fn extract_return_columns(query: &str) -> Result<Vec<String>, ParseError> {
    let pairs = CypherParser::parse(Rule::Cypher, query)
        .map_err(|e| ParseError::InvalidSyntax(format!("{}", e)))?;

    let mut columns = Vec::new();

    // Find all Return clauses and extract from the last one
    // (handles WITH clauses which also have ProjectionBody)
    extract_return_from_pairs(pairs, &mut columns)?;

    if columns.is_empty() {
        return Err(ParseError::NoReturnClause);
    }

    Ok(columns)
}

/// Recursively extracts return columns from parsed pairs.
fn extract_return_from_pairs(
    pairs: pest::iterators::Pairs<Rule>,
    columns: &mut Vec<String>,
) -> Result<(), ParseError> {
    for pair in pairs {
        match pair.as_rule() {
            Rule::Return => {
                // Clear previous columns (we want the final RETURN)
                columns.clear();
                extract_projection_items(pair.into_inner(), columns)?;
            }
            Rule::ProjectionItems => {
                // Check for RETURN *
                let text = pair.as_str().trim();
                if text.starts_with('*') {
                    return Err(ParseError::ReturnStarNotSupported);
                }
            }
            _ => {
                // Recurse into nested rules
                extract_return_from_pairs(pair.into_inner(), columns)?;
            }
        }
    }
    Ok(())
}

/// Extracts column names from ProjectionBody.
fn extract_projection_items(
    pairs: pest::iterators::Pairs<Rule>,
    columns: &mut Vec<String>,
) -> Result<(), ParseError> {
    for pair in pairs {
        match pair.as_rule() {
            Rule::ProjectionItems => {
                let text = pair.as_str().trim();
                if text.starts_with('*') {
                    return Err(ParseError::ReturnStarNotSupported);
                }
                extract_projection_items(pair.into_inner(), columns)?;
            }
            Rule::ProjectionItem => {
                let column = extract_column_name(pair)?;
                columns.push(column);
            }
            _ => {
                extract_projection_items(pair.into_inner(), columns)?;
            }
        }
    }
    Ok(())
}

/// Extracts the column name from a single ProjectionItem.
///
/// For `Expression AS Variable`, returns the Variable.
/// For `Expression` alone, returns the expression text.
fn extract_column_name(pair: pest::iterators::Pair<Rule>) -> Result<String, ParseError> {
    let full_text = pair.as_str();
    let mut expression_text = None;
    let mut alias = None;

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::Expression => {
                expression_text = Some(inner.as_str().trim().to_string());
            }
            Rule::Variable => {
                // This is the alias (comes after AS)
                alias = Some(extract_variable_name(inner));
            }
            _ => {}
        }
    }

    // Return alias if present, otherwise expression text
    Ok(alias.unwrap_or_else(|| expression_text.unwrap_or_else(|| full_text.trim().to_string())))
}

/// Extracts the variable name from a Variable node.
fn extract_variable_name(pair: pest::iterators::Pair<Rule>) -> String {
    // Variable contains SymbolicName
    let fallback = pair.as_str().to_string();
    for inner in pair.into_inner() {
        if inner.as_rule() == Rule::SymbolicName {
            let name = inner.as_str();
            // Handle escaped names (backticks)
            if name.starts_with('`') && name.ends_with('`') {
                return name[1..name.len() - 1].to_string();
            }
            return name.to_string();
        }
    }
    fallback
}

/// Errors that can occur during Cypher parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// No RETURN clause found in the query
    NoReturnClause,
    /// RETURN * requires variable tracking (not supported)
    ReturnStarNotSupported,
    /// Syntax error in the query
    InvalidSyntax(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::NoReturnClause => write!(f, "No RETURN clause found in query"),
            ParseError::ReturnStarNotSupported => {
                write!(
                    f,
                    "RETURN * is not supported - please specify columns explicitly"
                )
            }
            ParseError::InvalidSyntax(msg) => write!(f, "Invalid syntax: {}", msg),
        }
    }
}

impl std::error::Error for ParseError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_variable() {
        let cols = extract_return_columns("MATCH (n) RETURN n").unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    #[test]
    fn test_aliased_variable() {
        let cols = extract_return_columns("MATCH (n) RETURN n AS node").unwrap();
        assert_eq!(cols, vec!["node"]);
    }

    #[test]
    fn test_property_access() {
        let cols = extract_return_columns("MATCH (n) RETURN n.name").unwrap();
        assert_eq!(cols, vec!["n.name"]);
    }

    #[test]
    fn test_property_with_alias() {
        let cols = extract_return_columns("MATCH (n) RETURN n.name AS name").unwrap();
        assert_eq!(cols, vec!["name"]);
    }

    #[test]
    fn test_multiple_items() {
        let cols = extract_return_columns("MATCH (n) RETURN n.name, n.age, n.id").unwrap();
        assert_eq!(cols, vec!["n.name", "n.age", "n.id"]);
    }

    #[test]
    fn test_mixed_aliased_and_not() {
        let cols = extract_return_columns("RETURN a, r AS rel, b").unwrap();
        assert_eq!(cols, vec!["a", "rel", "b"]);
    }

    #[test]
    fn test_expression_with_arithmetic() {
        let cols = extract_return_columns("RETURN n.age + 10").unwrap();
        assert_eq!(cols, vec!["n.age + 10"]);
    }

    #[test]
    fn test_expression_with_alias() {
        let cols = extract_return_columns("RETURN n.age + 10 AS future_age").unwrap();
        assert_eq!(cols, vec!["future_age"]);
    }

    #[test]
    fn test_function_call() {
        let cols = extract_return_columns("RETURN count(n)").unwrap();
        assert_eq!(cols, vec!["count(n)"]);
    }

    #[test]
    fn test_function_with_alias() {
        let cols = extract_return_columns("RETURN count(n) AS total").unwrap();
        assert_eq!(cols, vec!["total"]);
    }

    #[test]
    fn test_nested_function() {
        let cols = extract_return_columns("RETURN collect(n.name)").unwrap();
        assert_eq!(cols, vec!["collect(n.name)"]);
    }

    #[test]
    fn test_case_expression() {
        let cols = extract_return_columns(
            "RETURN CASE WHEN n.age > 18 THEN 'adult' ELSE 'minor' END AS category",
        )
        .unwrap();
        assert_eq!(cols, vec!["category"]);
    }

    #[test]
    fn test_with_order_by() {
        let cols = extract_return_columns("MATCH (n) RETURN n.name, n.age ORDER BY n.age").unwrap();
        assert_eq!(cols, vec!["n.name", "n.age"]);
    }

    #[test]
    fn test_with_limit() {
        let cols = extract_return_columns("MATCH (n) RETURN n.name LIMIT 10").unwrap();
        assert_eq!(cols, vec!["n.name"]);
    }

    #[test]
    fn test_with_skip_limit() {
        let cols = extract_return_columns("MATCH (n) RETURN n.name SKIP 5 LIMIT 10").unwrap();
        assert_eq!(cols, vec!["n.name"]);
    }

    #[test]
    fn test_distinct() {
        let cols = extract_return_columns("MATCH (n) RETURN DISTINCT n.name").unwrap();
        assert_eq!(cols, vec!["n.name"]);
    }

    #[test]
    fn test_string_literal() {
        let cols = extract_return_columns("RETURN 'hello, world' AS greeting").unwrap();
        assert_eq!(cols, vec!["greeting"]);
    }

    #[test]
    fn test_string_with_return_keyword() {
        // This tests that RETURN inside a string doesn't confuse the parser
        let cols = extract_return_columns("MATCH (n) WHERE n.text = 'RETURN value' RETURN n.name")
            .unwrap();
        assert_eq!(cols, vec!["n.name"]);
    }

    #[test]
    fn test_list_expression() {
        let cols = extract_return_columns("RETURN [n.a, n.b, n.c] AS items").unwrap();
        assert_eq!(cols, vec!["items"]);
    }

    #[test]
    fn test_no_return_clause() {
        // A Cypher query without RETURN (or UPDATE) is actually invalid syntax,
        // so we get InvalidSyntax rather than NoReturnClause
        let result = extract_return_columns("MATCH (n) WHERE n.id = 1");
        assert!(result.is_err(), "Expected error for query without RETURN");
    }

    #[test]
    fn test_case_insensitive_return() {
        let cols = extract_return_columns("match (n) return n.name").unwrap();
        assert_eq!(cols, vec!["n.name"]);
    }

    #[test]
    fn test_case_insensitive_as() {
        let cols = extract_return_columns("RETURN n.name as name").unwrap();
        assert_eq!(cols, vec!["name"]);
    }

    #[test]
    fn test_complex_query() {
        let cols = extract_return_columns(
            "MATCH (a:Person)-[r:KNOWS]->(b:Person) WHERE a.name = 'Alice' RETURN a, r, b ORDER BY r.since"
        ).unwrap();
        assert_eq!(cols, vec!["a", "r", "b"]);
    }

    #[test]
    fn test_map_projection() {
        let cols = extract_return_columns("RETURN {name: n.name, age: n.age} AS data").unwrap();
        assert_eq!(cols, vec!["data"]);
    }

    #[test]
    fn test_backtick_identifier() {
        let cols = extract_return_columns("RETURN n.name AS `column name`").unwrap();
        assert_eq!(cols, vec!["column name"]);
    }

    #[test]
    fn test_with_clause_uses_last_return() {
        // WITH has projection too, but we want the final RETURN
        let cols = extract_return_columns(
            "MATCH (n) WITH n.name AS name WHERE name STARTS WITH 'A' RETURN name, count(*) AS cnt",
        )
        .unwrap();
        assert_eq!(cols, vec!["name", "cnt"]);
    }

    #[test]
    fn test_union_multiple_returns() {
        // UNION has multiple RETURNs - we get the last one (they should match anyway)
        let cols = extract_return_columns(
            "MATCH (a) RETURN a.name AS name UNION MATCH (b) RETURN b.name AS name",
        )
        .unwrap();
        assert_eq!(cols, vec!["name"]);
    }

    #[test]
    fn test_return_star_not_supported() {
        let result = extract_return_columns("MATCH (n) RETURN *");
        assert!(matches!(result, Err(ParseError::ReturnStarNotSupported)));
    }
}
