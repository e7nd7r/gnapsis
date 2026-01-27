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

    // =========================================================================
    // Grammar Branch Coverage Tests
    // =========================================================================

    // --- Union Variants ---

    #[test]
    fn test_union_all() {
        let cols = extract_return_columns(
            "MATCH (a) RETURN a.name AS name UNION ALL MATCH (b) RETURN b.name AS name",
        )
        .unwrap();
        assert_eq!(cols, vec!["name"]);
    }

    #[test]
    fn test_multiple_unions() {
        let cols = extract_return_columns(
            "MATCH (a) RETURN a.x UNION MATCH (b) RETURN b.x UNION ALL MATCH (c) RETURN c.x",
        )
        .unwrap();
        assert_eq!(cols, vec!["c.x"]);
    }

    // --- Reading Clauses ---

    #[test]
    fn test_optional_match() {
        let cols =
            extract_return_columns("MATCH (n) OPTIONAL MATCH (n)-[r]->(m) RETURN n, r, m").unwrap();
        assert_eq!(cols, vec!["n", "r", "m"]);
    }

    #[test]
    fn test_unwind() {
        let cols =
            extract_return_columns("UNWIND [1, 2, 3] AS x RETURN x, x * 2 AS doubled").unwrap();
        assert_eq!(cols, vec!["x", "doubled"]);
    }

    #[test]
    fn test_unwind_with_match() {
        let cols =
            extract_return_columns("MATCH (n) UNWIND n.tags AS tag RETURN n.name, tag").unwrap();
        assert_eq!(cols, vec!["n.name", "tag"]);
    }

    // --- Updating Clauses ---

    #[test]
    fn test_create_with_return() {
        let cols =
            extract_return_columns("CREATE (n:Person {name: 'Alice'}) RETURN n.name AS name")
                .unwrap();
        assert_eq!(cols, vec!["name"]);
    }

    #[test]
    fn test_merge_with_return() {
        let cols = extract_return_columns(
            "MERGE (n:Person {id: 1}) ON CREATE SET n.created = true ON MATCH SET n.updated = true RETURN n",
        )
        .unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    #[test]
    fn test_delete_with_return() {
        let cols = extract_return_columns("MATCH (n) DELETE n RETURN count(*) AS deleted").unwrap();
        assert_eq!(cols, vec!["deleted"]);
    }

    #[test]
    fn test_detach_delete() {
        let cols =
            extract_return_columns("MATCH (n) DETACH DELETE n RETURN count(*) AS deleted").unwrap();
        assert_eq!(cols, vec!["deleted"]);
    }

    #[test]
    fn test_set_property() {
        let cols = extract_return_columns("MATCH (n) SET n.updated = true RETURN n").unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    #[test]
    fn test_set_multiple_properties() {
        let cols =
            extract_return_columns("MATCH (n) SET n.a = 1, n.b = 2, n += {c: 3} RETURN n").unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    #[test]
    fn test_set_labels() {
        let cols = extract_return_columns("MATCH (n) SET n:Active:Verified RETURN n").unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    #[test]
    fn test_remove_property() {
        let cols = extract_return_columns("MATCH (n) REMOVE n.temp RETURN n").unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    #[test]
    fn test_remove_labels() {
        let cols = extract_return_columns("MATCH (n) REMOVE n:Temp, n:Draft RETURN n").unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    // --- Relationship Patterns (all 4 directions) ---

    #[test]
    fn test_relationship_right_arrow() {
        let cols = extract_return_columns("MATCH (a)-[r]->(b) RETURN a, r, b").unwrap();
        assert_eq!(cols, vec!["a", "r", "b"]);
    }

    #[test]
    fn test_relationship_left_arrow() {
        let cols = extract_return_columns("MATCH (a)<-[r]-(b) RETURN a, r, b").unwrap();
        assert_eq!(cols, vec!["a", "r", "b"]);
    }

    #[test]
    fn test_relationship_no_direction() {
        let cols = extract_return_columns("MATCH (a)-[r]-(b) RETURN a, r, b").unwrap();
        assert_eq!(cols, vec!["a", "r", "b"]);
    }

    #[test]
    fn test_relationship_bidirectional() {
        let cols = extract_return_columns("MATCH (a)<-[r]->(b) RETURN a, r, b").unwrap();
        assert_eq!(cols, vec!["a", "r", "b"]);
    }

    #[test]
    fn test_relationship_multiple_types() {
        let cols =
            extract_return_columns("MATCH (a)-[r:KNOWS|LIKES|FOLLOWS]->(b) RETURN r").unwrap();
        assert_eq!(cols, vec!["r"]);
    }

    #[test]
    fn test_relationship_with_properties() {
        let cols = extract_return_columns("MATCH (a)-[r:KNOWS {since: 2020}]->(b) RETURN r.since")
            .unwrap();
        assert_eq!(cols, vec!["r.since"]);
    }

    // --- Variable-length Paths ---

    #[test]
    fn test_variable_length_any() {
        let cols = extract_return_columns("MATCH (a)-[*]->(b) RETURN a, b").unwrap();
        assert_eq!(cols, vec!["a", "b"]);
    }

    #[test]
    fn test_variable_length_min() {
        let cols = extract_return_columns("MATCH (a)-[*2..]->(b) RETURN a, b").unwrap();
        assert_eq!(cols, vec!["a", "b"]);
    }

    #[test]
    fn test_variable_length_max() {
        let cols = extract_return_columns("MATCH (a)-[*..5]->(b) RETURN a, b").unwrap();
        assert_eq!(cols, vec!["a", "b"]);
    }

    #[test]
    fn test_variable_length_range() {
        let cols = extract_return_columns("MATCH (a)-[*2..5]->(b) RETURN a, b").unwrap();
        assert_eq!(cols, vec!["a", "b"]);
    }

    #[test]
    fn test_variable_length_exact() {
        let cols = extract_return_columns("MATCH (a)-[*3]->(b) RETURN a, b").unwrap();
        assert_eq!(cols, vec!["a", "b"]);
    }

    // --- Node Patterns ---

    #[test]
    fn test_multiple_labels() {
        let cols = extract_return_columns("MATCH (n:Person:Employee:Manager) RETURN n").unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    #[test]
    fn test_node_with_properties() {
        let cols = extract_return_columns(
            "MATCH (n:Person {name: 'Alice', age: 30, active: true}) RETURN n",
        )
        .unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    #[test]
    fn test_node_with_parameter_properties() {
        let cols = extract_return_columns("MATCH (n:Person $props) RETURN n").unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    #[test]
    fn test_named_pattern() {
        let cols =
            extract_return_columns("MATCH p = (a)-[r]->(b) RETURN p, length(p) AS len").unwrap();
        assert_eq!(cols, vec!["p", "len"]);
    }

    #[test]
    fn test_parenthesized_pattern() {
        let cols = extract_return_columns("MATCH ((a)-[r]->(b)) RETURN a, b").unwrap();
        assert_eq!(cols, vec!["a", "b"]);
    }

    // --- Boolean Operators ---

    #[test]
    fn test_or_expression() {
        let cols = extract_return_columns("MATCH (n) WHERE n.a = 1 OR n.b = 2 RETURN n").unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    #[test]
    fn test_xor_expression() {
        let cols = extract_return_columns("MATCH (n) WHERE n.a = 1 XOR n.b = 2 RETURN n").unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    #[test]
    fn test_and_expression() {
        let cols = extract_return_columns("MATCH (n) WHERE n.a = 1 AND n.b = 2 RETURN n").unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    #[test]
    fn test_not_expression() {
        let cols = extract_return_columns("MATCH (n) WHERE NOT n.deleted RETURN n").unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    #[test]
    fn test_double_not() {
        let cols = extract_return_columns("MATCH (n) WHERE NOT NOT n.active RETURN n").unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    #[test]
    fn test_complex_boolean() {
        let cols =
            extract_return_columns("MATCH (n) WHERE (n.a OR n.b) AND NOT (n.c XOR n.d) RETURN n")
                .unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    // --- Comparison Operators ---

    #[test]
    fn test_comparison_equal() {
        let cols = extract_return_columns("MATCH (n) WHERE n.x = 1 RETURN n").unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    #[test]
    fn test_comparison_not_equal() {
        let cols = extract_return_columns("MATCH (n) WHERE n.x <> 1 RETURN n").unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    #[test]
    fn test_comparison_less_than() {
        let cols = extract_return_columns("MATCH (n) WHERE n.x < 10 RETURN n").unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    #[test]
    fn test_comparison_greater_than() {
        let cols = extract_return_columns("MATCH (n) WHERE n.x > 10 RETURN n").unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    #[test]
    fn test_comparison_less_equal() {
        let cols = extract_return_columns("MATCH (n) WHERE n.x <= 10 RETURN n").unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    #[test]
    fn test_comparison_greater_equal() {
        let cols = extract_return_columns("MATCH (n) WHERE n.x >= 10 RETURN n").unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    #[test]
    fn test_chained_comparison() {
        let cols = extract_return_columns("MATCH (n) WHERE 0 < n.x <= 100 RETURN n").unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    // --- Arithmetic Operators ---

    #[test]
    fn test_arithmetic_subtraction() {
        let cols = extract_return_columns("RETURN 10 - 3 AS result").unwrap();
        assert_eq!(cols, vec!["result"]);
    }

    #[test]
    fn test_arithmetic_multiplication() {
        let cols = extract_return_columns("RETURN 5 * 3 AS result").unwrap();
        assert_eq!(cols, vec!["result"]);
    }

    #[test]
    fn test_arithmetic_division() {
        let cols = extract_return_columns("RETURN 10 / 2 AS result").unwrap();
        assert_eq!(cols, vec!["result"]);
    }

    #[test]
    fn test_arithmetic_modulo() {
        let cols = extract_return_columns("RETURN 10 % 3 AS result").unwrap();
        assert_eq!(cols, vec!["result"]);
    }

    #[test]
    fn test_arithmetic_power() {
        let cols = extract_return_columns("RETURN 2 ^ 10 AS result").unwrap();
        assert_eq!(cols, vec!["result"]);
    }

    #[test]
    fn test_unary_minus() {
        let cols = extract_return_columns("RETURN -n.value AS negated").unwrap();
        assert_eq!(cols, vec!["negated"]);
    }

    #[test]
    fn test_unary_plus() {
        let cols = extract_return_columns("RETURN +n.value AS positive").unwrap();
        assert_eq!(cols, vec!["positive"]);
    }

    #[test]
    fn test_complex_arithmetic() {
        let cols = extract_return_columns("RETURN (a + b) * c - d / e % f ^ g AS result").unwrap();
        assert_eq!(cols, vec!["result"]);
    }

    // --- String Operators ---

    #[test]
    fn test_starts_with() {
        let cols =
            extract_return_columns("MATCH (n) WHERE n.name STARTS WITH 'A' RETURN n").unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    #[test]
    fn test_ends_with() {
        let cols =
            extract_return_columns("MATCH (n) WHERE n.name ENDS WITH 'son' RETURN n").unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    #[test]
    fn test_contains() {
        let cols = extract_return_columns("MATCH (n) WHERE n.name CONTAINS 'li' RETURN n").unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    // --- List Operators ---

    #[test]
    fn test_in_list() {
        let cols =
            extract_return_columns("MATCH (n) WHERE n.status IN ['active', 'pending'] RETURN n")
                .unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    #[test]
    fn test_list_index() {
        let cols = extract_return_columns("RETURN [1, 2, 3][0] AS first").unwrap();
        assert_eq!(cols, vec!["first"]);
    }

    #[test]
    fn test_list_slice_both() {
        let cols = extract_return_columns("RETURN [1, 2, 3, 4, 5][1..3] AS slice").unwrap();
        assert_eq!(cols, vec!["slice"]);
    }

    #[test]
    fn test_list_slice_from() {
        let cols = extract_return_columns("RETURN [1, 2, 3, 4, 5][2..] AS tail").unwrap();
        assert_eq!(cols, vec!["tail"]);
    }

    #[test]
    fn test_list_slice_to() {
        let cols = extract_return_columns("RETURN [1, 2, 3, 4, 5][..3] AS head").unwrap();
        assert_eq!(cols, vec!["head"]);
    }

    // --- Null Operators ---

    #[test]
    fn test_is_null() {
        let cols = extract_return_columns("MATCH (n) WHERE n.deleted IS NULL RETURN n").unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    #[test]
    fn test_is_not_null() {
        let cols = extract_return_columns("MATCH (n) WHERE n.email IS NOT NULL RETURN n").unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    // --- Literals ---

    #[test]
    fn test_hex_integer() {
        let cols = extract_return_columns("RETURN 0xFF AS hex").unwrap();
        assert_eq!(cols, vec!["hex"]);
    }

    #[test]
    fn test_octal_integer() {
        let cols = extract_return_columns("RETURN 0777 AS octal").unwrap();
        assert_eq!(cols, vec!["octal"]);
    }

    #[test]
    fn test_double_literal() {
        let cols = extract_return_columns("RETURN 3.14159 AS pi").unwrap();
        assert_eq!(cols, vec!["pi"]);
    }

    #[test]
    fn test_scientific_notation() {
        let cols = extract_return_columns("RETURN 1.5e10 AS big").unwrap();
        assert_eq!(cols, vec!["big"]);
    }

    #[test]
    fn test_scientific_notation_negative() {
        let cols = extract_return_columns("RETURN 2.5E-3 AS small").unwrap();
        assert_eq!(cols, vec!["small"]);
    }

    #[test]
    fn test_boolean_true() {
        let cols = extract_return_columns("RETURN true AS flag").unwrap();
        assert_eq!(cols, vec!["flag"]);
    }

    #[test]
    fn test_boolean_false() {
        let cols = extract_return_columns("RETURN false AS flag").unwrap();
        assert_eq!(cols, vec!["flag"]);
    }

    #[test]
    fn test_null_literal() {
        let cols = extract_return_columns("RETURN null AS nothing").unwrap();
        assert_eq!(cols, vec!["nothing"]);
    }

    #[test]
    fn test_string_double_quotes() {
        let cols = extract_return_columns("RETURN \"hello\" AS greeting").unwrap();
        assert_eq!(cols, vec!["greeting"]);
    }

    #[test]
    fn test_string_escaped_chars() {
        let cols = extract_return_columns(r#"RETURN 'line1\nline2\ttab\\slash' AS text"#).unwrap();
        assert_eq!(cols, vec!["text"]);
    }

    #[test]
    fn test_string_unicode_escape() {
        let cols = extract_return_columns(r"RETURN '\u0041\u0042' AS ab").unwrap();
        assert_eq!(cols, vec!["ab"]);
    }

    #[test]
    fn test_empty_list() {
        let cols = extract_return_columns("RETURN [] AS empty").unwrap();
        assert_eq!(cols, vec!["empty"]);
    }

    #[test]
    fn test_empty_map() {
        let cols = extract_return_columns("RETURN {} AS empty").unwrap();
        assert_eq!(cols, vec!["empty"]);
    }

    #[test]
    fn test_nested_map() {
        let cols =
            extract_return_columns("RETURN {outer: {inner: {deep: 'value'}}} AS nested").unwrap();
        assert_eq!(cols, vec!["nested"]);
    }

    // --- Parameters ---

    #[test]
    fn test_named_parameter() {
        let cols = extract_return_columns("MATCH (n) WHERE n.id = $userId RETURN n").unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    #[test]
    fn test_numbered_parameter() {
        let cols = extract_return_columns("MATCH (n) WHERE n.id = $0 RETURN n").unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    // --- CASE Expressions ---

    #[test]
    fn test_simple_case() {
        let cols = extract_return_columns(
            "RETURN CASE n.status WHEN 'active' THEN 1 WHEN 'pending' THEN 2 ELSE 0 END AS code",
        )
        .unwrap();
        assert_eq!(cols, vec!["code"]);
    }

    #[test]
    fn test_searched_case_no_else() {
        let cols = extract_return_columns(
            "RETURN CASE WHEN n.age < 18 THEN 'minor' WHEN n.age < 65 THEN 'adult' END AS category",
        )
        .unwrap();
        assert_eq!(cols, vec!["category"]);
    }

    #[test]
    fn test_case_multiple_when() {
        let cols = extract_return_columns(
            "RETURN CASE WHEN a THEN 1 WHEN b THEN 2 WHEN c THEN 3 ELSE 0 END AS val",
        )
        .unwrap();
        assert_eq!(cols, vec!["val"]);
    }

    // --- Filter Expressions ---

    #[test]
    fn test_all_predicate() {
        let cols =
            extract_return_columns("MATCH (n) WHERE all(x IN n.scores WHERE x > 50) RETURN n")
                .unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    #[test]
    fn test_any_predicate() {
        let cols = extract_return_columns(
            "MATCH (n) WHERE any(x IN n.tags WHERE x = 'important') RETURN n",
        )
        .unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    #[test]
    fn test_none_predicate() {
        let cols = extract_return_columns(
            "MATCH (n) WHERE none(x IN n.flags WHERE x = 'deleted') RETURN n",
        )
        .unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    #[test]
    fn test_single_predicate() {
        let cols = extract_return_columns(
            "MATCH (n) WHERE single(x IN n.admins WHERE x = 'root') RETURN n",
        )
        .unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    // --- Comprehensions ---

    #[test]
    fn test_list_comprehension() {
        let cols = extract_return_columns("RETURN [x IN range(1, 10) | x * 2] AS doubled").unwrap();
        assert_eq!(cols, vec!["doubled"]);
    }

    #[test]
    fn test_list_comprehension_with_filter() {
        let cols =
            extract_return_columns("RETURN [x IN range(1, 10) WHERE x % 2 = 0 | x * x] AS squares")
                .unwrap();
        assert_eq!(cols, vec!["squares"]);
    }

    #[test]
    fn test_list_comprehension_no_map() {
        let cols =
            extract_return_columns("RETURN [x IN range(1, 10) WHERE x > 5] AS filtered").unwrap();
        assert_eq!(cols, vec!["filtered"]);
    }

    #[test]
    fn test_pattern_comprehension() {
        let cols =
            extract_return_columns("MATCH (n) RETURN [p = (n)-[:KNOWS]->(m) | m.name] AS friends")
                .unwrap();
        assert_eq!(cols, vec!["friends"]);
    }

    #[test]
    fn test_pattern_comprehension_with_where() {
        let cols = extract_return_columns(
            "MATCH (n) RETURN [(n)-[:KNOWS]->(m) WHERE m.age > 21 | m.name] AS adult_friends",
        )
        .unwrap();
        assert_eq!(cols, vec!["adult_friends"]);
    }

    // --- Functions ---

    #[test]
    fn test_count_star() {
        let cols = extract_return_columns("MATCH (n) RETURN count(*) AS total").unwrap();
        assert_eq!(cols, vec!["total"]);
    }

    #[test]
    fn test_function_distinct() {
        let cols =
            extract_return_columns("MATCH (n) RETURN count(DISTINCT n.category) AS categories")
                .unwrap();
        assert_eq!(cols, vec!["categories"]);
    }

    #[test]
    fn test_exists_function() {
        let cols = extract_return_columns("MATCH (n) WHERE exists(n.email) RETURN n").unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    #[test]
    fn test_namespaced_function() {
        let cols = extract_return_columns("RETURN apoc.text.capitalize('hello') AS cap").unwrap();
        assert_eq!(cols, vec!["cap"]);
    }

    #[test]
    fn test_deeply_namespaced_function() {
        let cols = extract_return_columns("RETURN a.b.c.d.function(x) AS result").unwrap();
        assert_eq!(cols, vec!["result"]);
    }

    // --- Procedure Calls ---

    #[test]
    fn test_standalone_call() {
        let cols = extract_return_columns("CALL db.labels() YIELD label RETURN label").unwrap();
        assert_eq!(cols, vec!["label"]);
    }

    #[test]
    fn test_call_with_args() {
        let cols = extract_return_columns(
            "CALL db.index.fulltext.queryNodes('myIndex', 'search') YIELD node RETURN node",
        )
        .unwrap();
        assert_eq!(cols, vec!["node"]);
    }

    #[test]
    fn test_call_yield_multiple() {
        let cols = extract_return_columns(
            "CALL dbms.listConfig() YIELD name, value WHERE name STARTS WITH 'db' RETURN name, value",
        )
        .unwrap();
        assert_eq!(cols, vec!["name", "value"]);
    }

    #[test]
    fn test_call_yield_alias() {
        let cols =
            extract_return_columns("CALL db.labels() YIELD label AS lbl RETURN lbl").unwrap();
        assert_eq!(cols, vec!["lbl"]);
    }

    #[test]
    fn test_in_query_call() {
        let cols = extract_return_columns("MATCH (n) CALL db.labels() YIELD label RETURN n, label")
            .unwrap();
        assert_eq!(cols, vec!["n", "label"]);
    }

    // --- ORDER BY Variants ---

    #[test]
    fn test_order_by_asc() {
        let cols = extract_return_columns("MATCH (n) RETURN n.name ORDER BY n.name ASC").unwrap();
        assert_eq!(cols, vec!["n.name"]);
    }

    #[test]
    fn test_order_by_ascending() {
        let cols =
            extract_return_columns("MATCH (n) RETURN n.name ORDER BY n.name ASCENDING").unwrap();
        assert_eq!(cols, vec!["n.name"]);
    }

    #[test]
    fn test_order_by_desc() {
        let cols = extract_return_columns("MATCH (n) RETURN n.name ORDER BY n.name DESC").unwrap();
        assert_eq!(cols, vec!["n.name"]);
    }

    #[test]
    fn test_order_by_descending() {
        let cols =
            extract_return_columns("MATCH (n) RETURN n.name ORDER BY n.name DESCENDING").unwrap();
        assert_eq!(cols, vec!["n.name"]);
    }

    #[test]
    fn test_order_by_multiple() {
        let cols = extract_return_columns(
            "MATCH (n) RETURN n ORDER BY n.lastName ASC, n.firstName DESC, n.age",
        )
        .unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    // --- WITH Clause Variants ---

    #[test]
    fn test_with_where() {
        let cols =
            extract_return_columns("MATCH (n) WITH n WHERE n.active = true RETURN n").unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    #[test]
    fn test_with_distinct() {
        let cols = extract_return_columns(
            "MATCH (n) WITH DISTINCT n.category AS cat RETURN cat, count(*) AS cnt",
        )
        .unwrap();
        assert_eq!(cols, vec!["cat", "cnt"]);
    }

    #[test]
    fn test_with_order_skip_limit() {
        let cols = extract_return_columns(
            "MATCH (n) WITH n ORDER BY n.score DESC SKIP 10 LIMIT 5 RETURN n.name",
        )
        .unwrap();
        assert_eq!(cols, vec!["n.name"]);
    }

    #[test]
    fn test_multi_part_query() {
        let cols =
            extract_return_columns("MATCH (a) WITH a MATCH (b) WITH a, b MATCH (c) RETURN a, b, c")
                .unwrap();
        assert_eq!(cols, vec!["a", "b", "c"]);
    }

    // --- Identifiers ---

    #[test]
    fn test_reserved_word_as_identifier() {
        // Using reserved words as property/label names
        let cols = extract_return_columns("MATCH (n:Match) RETURN n.return AS `order`").unwrap();
        assert_eq!(cols, vec!["order"]);
    }

    #[test]
    fn test_backtick_with_special_chars() {
        let cols = extract_return_columns("RETURN n.`first name` AS `full-name`").unwrap();
        assert_eq!(cols, vec!["full-name"]);
    }

    #[test]
    fn test_unicode_identifier() {
        let cols = extract_return_columns("MATCH (nœud) RETURN nœud.prénom AS nom").unwrap();
        assert_eq!(cols, vec!["nom"]);
    }

    // --- Comments ---

    #[test]
    fn test_line_comment() {
        let cols = extract_return_columns("MATCH (n) // this is a comment\nRETURN n.name").unwrap();
        assert_eq!(cols, vec!["n.name"]);
    }

    #[test]
    fn test_block_comment() {
        let cols =
            extract_return_columns("MATCH (n) /* block comment */ RETURN /* another */ n.name")
                .unwrap();
        assert_eq!(cols, vec!["n.name"]);
    }

    #[test]
    fn test_multiline_block_comment() {
        let cols =
            extract_return_columns("MATCH (n)\n/* this is a\nmultiline\ncomment */\nRETURN n")
                .unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    // --- Parenthesized Expressions ---

    #[test]
    fn test_parenthesized_expression() {
        let cols = extract_return_columns("RETURN (1 + 2) * 3 AS result").unwrap();
        assert_eq!(cols, vec!["result"]);
    }

    #[test]
    fn test_deeply_nested_parens() {
        let cols = extract_return_columns("RETURN (((a + b))) AS result").unwrap();
        assert_eq!(cols, vec!["result"]);
    }

    // --- Relationships Pattern in Expression ---

    #[test]
    fn test_exists_pattern() {
        let cols = extract_return_columns("MATCH (n) WHERE (n)-[:KNOWS]->() RETURN n").unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    #[test]
    fn test_pattern_in_expression() {
        let cols =
            extract_return_columns("MATCH (n) RETURN (n)-[:FRIEND]->(m) AS has_friend").unwrap();
        assert_eq!(cols, vec!["has_friend"]);
    }

    // --- Edge Cases ---

    #[test]
    fn test_empty_node_pattern() {
        let cols = extract_return_columns("MATCH () RETURN count(*) AS cnt").unwrap();
        assert_eq!(cols, vec!["cnt"]);
    }

    #[test]
    fn test_long_chain() {
        let cols =
            extract_return_columns("MATCH (a)-[r1]->(b)-[r2]->(c)-[r3]->(d) RETURN a, d").unwrap();
        assert_eq!(cols, vec!["a", "d"]);
    }

    #[test]
    fn test_multiple_patterns() {
        let cols = extract_return_columns("MATCH (a), (b), (a)-[r]->(b) RETURN a, r, b").unwrap();
        assert_eq!(cols, vec!["a", "r", "b"]);
    }

    #[test]
    fn test_trailing_semicolon() {
        let cols = extract_return_columns("MATCH (n) RETURN n;").unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    #[test]
    fn test_leading_whitespace() {
        let cols = extract_return_columns("   \n\t  MATCH (n) RETURN n").unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    #[test]
    fn test_return_distinct_with_order_skip_limit() {
        let cols = extract_return_columns(
            "MATCH (n) RETURN DISTINCT n.cat AS cat ORDER BY cat SKIP 5 LIMIT 10",
        )
        .unwrap();
        assert_eq!(cols, vec!["cat"]);
    }

    #[test]
    fn test_deeply_nested_properties() {
        let cols = extract_return_columns("RETURN n.a.b.c.d.e AS deep").unwrap();
        assert_eq!(cols, vec!["deep"]);
    }

    #[test]
    fn test_labels_in_expression() {
        let cols = extract_return_columns("MATCH (n) WHERE n:Person:Employee RETURN n").unwrap();
        assert_eq!(cols, vec!["n"]);
    }

    #[test]
    fn test_zero_literal() {
        let cols = extract_return_columns("RETURN 0 AS zero").unwrap();
        assert_eq!(cols, vec!["zero"]);
    }

    #[test]
    fn test_decimal_starting_with_dot() {
        let cols = extract_return_columns("RETURN .5 AS half").unwrap();
        assert_eq!(cols, vec!["half"]);
    }
}
