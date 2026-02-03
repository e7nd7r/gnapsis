//! ECS components for graph visualization.
//!
//! Components are data attached to entities. Each entity can have
//! any combination of components.

use bevy::prelude::*;

/// Marker component for graph node entities.
///
/// Attached to sphere/cube meshes representing nodes in the graph.
#[derive(Component)]
pub struct GraphNode {
    /// Unique identifier matching the layout node.
    pub id: String,
    /// Index in the layout's node array.
    pub node_idx: usize,
    /// Visual radius of this node.
    pub radius: f32,
}

/// Marker component for edge line entities.
///
/// Attached to cylinder meshes representing edges between nodes.
#[derive(Component)]
pub struct GraphEdge {
    /// Index of the source node.
    pub from_idx: usize,
    /// Index of the target node.
    pub to_idx: usize,
    /// Relationship type (e.g., "CALLS", "BELONGS_TO").
    pub relationship: String,
}

/// Label component that follows a node in screen space.
///
/// Attached to UI text entities that display node names.
#[derive(Component)]
pub struct NodeLabel {
    /// Index of the node this label follows.
    pub node_idx: usize,
}

/// Invisible hotspot for edge click detection.
///
/// Positioned at edge midpoints to enable edge selection.
#[derive(Component)]
pub struct EdgeHotspot {
    /// Index of the source node.
    pub from_idx: usize,
    /// Index of the target node.
    pub to_idx: usize,
    /// Relationship type for display.
    pub relationship: String,
    /// Optional note on the relationship.
    pub note: Option<String>,
}

/// Arrowhead cone showing edge direction.
///
/// Positioned near the target node to indicate relationship direction.
#[derive(Component)]
pub struct EdgeArrow {
    /// Index of the source node.
    pub from_idx: usize,
    /// Index of the target node.
    pub to_idx: usize,
}

/// Marker component for the info panel container.
#[derive(Component)]
pub struct InfoPanel;

/// Marker component for the info panel text content.
#[derive(Component)]
pub struct InfoPanelText;
