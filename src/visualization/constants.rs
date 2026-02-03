//! Visual and physics constants for the graph visualization.

use bevy::prelude::*;

// =============================================================================
// Node Colors by Scope
// =============================================================================

/// Domain scope color (Red-Coral).
pub const COLOR_DOMAIN: Color = Color::srgb(0.90, 0.35, 0.30);
/// Feature scope color (Amber).
pub const COLOR_FEATURE: Color = Color::srgb(0.95, 0.65, 0.15);
/// Namespace scope color (Teal).
pub const COLOR_NAMESPACE: Color = Color::srgb(0.25, 0.75, 0.70);
/// Component scope color (Blue-Violet).
pub const COLOR_COMPONENT: Color = Color::srgb(0.45, 0.50, 0.90);
/// Unit scope color (Silver-Lavender).
pub const COLOR_UNIT: Color = Color::srgb(0.65, 0.60, 0.75);
/// Default node color for unknown/missing scope (Blue).
pub const COLOR_NODE_DEFAULT: Color = Color::srgb(0.29, 0.56, 0.85);
/// Start/root node color (Bright White).
pub const COLOR_START: Color = Color::srgb(0.95, 0.95, 1.0);

/// All scope names in canonical order.
pub const SCOPE_NAMES: &[&str] = &["Domain", "Feature", "Namespace", "Component", "Unit"];

// =============================================================================
// Edge Colors by Relationship Type
// =============================================================================

/// BELONGS_TO relationship color (Vivid Purple).
pub const COLOR_BELONGS_TO: Color = Color::srgb(0.7, 0.3, 0.9);
/// CALLS relationship color (Orange).
pub const COLOR_CALLS: Color = Color::srgb(1.0, 0.5, 0.3);
/// IMPORTS relationship color (Yellow).
pub const COLOR_IMPORTS: Color = Color::srgb(0.9, 0.7, 0.2);
/// IMPLEMENTS relationship color (Pink).
pub const COLOR_IMPLEMENTS: Color = Color::srgb(0.8, 0.3, 0.5);
/// INSTANTIATES relationship color (Light green).
pub const COLOR_INSTANTIATES: Color = Color::srgb(0.5, 0.8, 0.4);
/// RELATED_TO relationship color (Gray).
pub const COLOR_RELATED_TO: Color = Color::srgb(0.7, 0.7, 0.7);
/// Default edge color (Gray).
pub const COLOR_EDGE_DEFAULT: Color = Color::srgb(0.5, 0.5, 0.5);

// =============================================================================
// Node Sizing
// =============================================================================

/// Base node radius (scaled by mass).
pub const BASE_NODE_RADIUS: f32 = 0.2;
/// Minimum node radius regardless of mass.
pub const MIN_NODE_RADIUS: f32 = 0.15;
/// Maximum node radius regardless of mass.
pub const MAX_NODE_RADIUS: f32 = 0.8;

// =============================================================================
// Helpers
// =============================================================================

/// Get node color for a scope level.
pub fn node_color_for_scope(scope: Option<&str>) -> Color {
    match scope {
        Some("Domain") => COLOR_DOMAIN,
        Some("Feature") => COLOR_FEATURE,
        Some("Namespace") => COLOR_NAMESPACE,
        Some("Component") => COLOR_COMPONENT,
        Some("Unit") => COLOR_UNIT,
        _ => COLOR_NODE_DEFAULT,
    }
}

/// Get color for a relationship type.
pub fn edge_color_for_relationship(relationship: &str) -> Color {
    match relationship {
        "BELONGS_TO" => COLOR_BELONGS_TO,
        "CALLS" => COLOR_CALLS,
        "IMPORTS" => COLOR_IMPORTS,
        "IMPLEMENTS" => COLOR_IMPLEMENTS,
        "INSTANTIATES" => COLOR_INSTANTIATES,
        "RELATED_TO" => COLOR_RELATED_TO,
        _ => COLOR_EDGE_DEFAULT,
    }
}
