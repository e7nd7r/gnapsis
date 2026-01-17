//! MCP tool response helpers.

use rmcp::model::CallToolResult;
use serde::Serialize;

/// Single-item response that serializes as the raw inner value.
///
/// Use this for tool responses that return a single object.
/// The inner value is serialized directly without wrapping.
///
/// # Example
///
/// ```ignore
/// let entity = Entity { id: "123", name: "Foo" };
/// Response(entity).into()
/// // Serializes as: { "id": "123", "name": "Foo" }
/// ```
pub struct Response<T>(pub T);

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
        Ok(CallToolResult::success(vec![rmcp::model::Content::json(
            serde_json::to_value(response).unwrap(),
        )
        .unwrap()]))
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
