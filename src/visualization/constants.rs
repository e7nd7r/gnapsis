//! Visual and physics constants for the graph visualization.

use bevy::prelude::*;

// =============================================================================
// Node Colors
// =============================================================================

/// Entity node color (Blue).
pub const COLOR_ENTITY: Color = Color::srgb(0.29, 0.56, 0.85); // #4A90D9
/// Document reference node color (Green).
pub const COLOR_DOCREF: Color = Color::srgb(0.36, 0.72, 0.36); // #5CB85C
/// Start/root node color (Gold).
pub const COLOR_START: Color = Color::srgb(1.0, 0.84, 0.0); // #FFD700

// =============================================================================
// Edge Colors by Relationship Type
// =============================================================================

/// BELONGS_TO relationship color (Purple).
pub const COLOR_BELONGS_TO: Color = Color::srgb(0.6, 0.4, 0.8);
/// CALLS relationship color (Orange).
pub const COLOR_CALLS: Color = Color::srgb(1.0, 0.5, 0.3);
/// HAS_REFERENCE relationship color (Cyan).
pub const COLOR_HAS_REFERENCE: Color = Color::srgb(0.3, 0.7, 0.9);
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

/// Get color for a relationship type.
pub fn edge_color_for_relationship(relationship: &str) -> Color {
    match relationship {
        "BELONGS_TO" => COLOR_BELONGS_TO,
        "CALLS" => COLOR_CALLS,
        "HAS_REFERENCE" => COLOR_HAS_REFERENCE,
        "IMPORTS" => COLOR_IMPORTS,
        "IMPLEMENTS" => COLOR_IMPLEMENTS,
        "INSTANTIATES" => COLOR_INSTANTIATES,
        "RELATED_TO" => COLOR_RELATED_TO,
        _ => COLOR_EDGE_DEFAULT,
    }
}
