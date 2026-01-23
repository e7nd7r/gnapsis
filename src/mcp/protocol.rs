//! MCP protocol response helpers.

use rmcp::model::CallToolResult;
use rmcp::schemars::{self, JsonSchema};
use serde::{Deserialize, Serialize};

/// Output format for tool responses.
#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    /// JSON format (default).
    #[default]
    Json,
    /// TOON (Token-Oriented Object Notation) - 40-60% fewer tokens.
    Toon,
}

/// Single-item response that serializes as the raw inner value.
///
/// Use this for tool responses that return a single object.
/// The inner value is serialized directly without wrapping.
///
/// # Example
///
/// ```ignore
/// let entity = Entity { id: "123", name: "Foo" };
/// Response(entity).into()  // JSON output (default)
/// Response(entity, Some(OutputFormat::Toon)).into()  // TOON output
/// ```
pub struct Response<T>(pub T, pub Option<OutputFormat>);

impl<T> Response<T> {
    /// Create a response with default (JSON) format.
    pub fn json(data: T) -> Self {
        Response(data, None)
    }

    /// Create a response with TOON format.
    pub fn toon(data: T) -> Self {
        Response(data, Some(OutputFormat::Toon))
    }
}

impl<T: Serialize> Serialize for Response<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<T: Serialize> From<Response<T>> for Result<CallToolResult, rmcp::model::ErrorData> {
    fn from(response: Response<T>) -> Self {
        match response.1.unwrap_or_default() {
            OutputFormat::Json => Ok(CallToolResult::success(vec![rmcp::model::Content::json(
                serde_json::to_value(&response.0).unwrap(),
            )
            .unwrap()])),
            OutputFormat::Toon => {
                let toon_str = serde_toon::to_string(&response.0)
                    .unwrap_or_else(|e| format!("TOON serialization error: {}", e));
                Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                    toon_str,
                )]))
            }
        }
    }
}

/// Paginated response with data array and pagination metadata.
///
/// Use this for tool responses that return lists of items.
///
/// # Example
///
/// ```ignore
/// PaginatedResponse {
///     data: entities,
///     pagination: Pagination {
///         total: 100,
///         offset: 0,
///         limit: 20,
///         has_more: true,
///     },
/// }.into()
/// ```
#[derive(Serialize)]
pub struct PaginatedResponse<T: Serialize> {
    /// The items for this page.
    pub data: Vec<T>,
    /// Pagination metadata.
    pub pagination: Pagination,
}

/// Pagination metadata for list responses.
#[derive(Serialize)]
pub struct Pagination {
    /// Total number of items across all pages.
    pub total: usize,
    /// Offset of the first item in this page.
    pub offset: usize,
    /// Maximum number of items per page.
    pub limit: usize,
    /// Whether there are more items after this page.
    pub has_more: bool,
}

impl<T: Serialize> From<PaginatedResponse<T>> for Result<CallToolResult, rmcp::model::ErrorData> {
    fn from(response: PaginatedResponse<T>) -> Self {
        Ok(CallToolResult::success(vec![rmcp::model::Content::json(
            serde_json::to_value(response).unwrap(),
        )
        .unwrap()]))
    }
}
