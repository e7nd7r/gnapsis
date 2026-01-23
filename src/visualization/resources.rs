//! ECS resources for graph visualization state.
//!
//! Resources are global singleton data - there's only one instance
//! of each resource in the entire app.

use bevy::prelude::*;
use std::collections::HashMap;
use std::sync::Mutex;

use super::graph::GraphLayout;
use super::nvim::NvimClient;

// =============================================================================
// Camera State
// =============================================================================

/// Camera orbit state for 3D navigation.
#[derive(Resource)]
pub struct CameraOrbit {
    /// Horizontal rotation angle (radians).
    pub yaw: f32,
    /// Vertical rotation angle (radians).
    pub pitch: f32,
    /// Distance from target.
    pub distance: f32,
    /// Point the camera orbits around.
    pub target: Vec3,
}

impl Default for CameraOrbit {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.3,
            distance: 25.0,
            target: Vec3::ZERO,
        }
    }
}

// =============================================================================
// Interaction State
// =============================================================================

/// State for node dragging operations.
#[derive(Resource, Default)]
pub struct DragState {
    /// Entity currently being dragged (if any).
    pub dragging: Option<Entity>,
    /// Node index in layout being dragged.
    pub node_idx: Option<usize>,
    /// Plane distance from camera for drag projection.
    pub drag_depth: f32,
    /// Total mouse movement during drag (to detect click vs drag).
    pub total_movement: f32,
    /// Offset from cursor to node center (prevents jumping).
    pub grab_offset: Vec3,
}

/// What is currently selected in the graph.
#[derive(Clone, Default, Debug)]
pub enum Selection {
    /// Nothing selected.
    #[default]
    None,
    /// A node is selected (by index).
    Node(usize),
    /// An edge is selected (by endpoint indices).
    Edge { from_idx: usize, to_idx: usize },
}

/// Currently selected element (node or edge).
#[derive(Resource, Default)]
pub struct CurrentSelection {
    pub selection: Selection,
}

// =============================================================================
// Graph Data
// =============================================================================

/// The graph layout containing node positions and physics state.
#[derive(Resource)]
pub struct GraphLayoutRes(pub GraphLayout);

// =============================================================================
// External Integrations
// =============================================================================

/// Neovim client for opening files (optional).
///
/// Wrapped in Mutex because Bevy resources must be Send + Sync,
/// and NvimClient contains a UnixStream.
#[derive(Resource)]
pub struct NvimClientRes(pub Mutex<Option<NvimClient>>);

// =============================================================================
// Materials
// =============================================================================

/// Pre-created materials for nodes and edges.
///
/// Storing material handles in a resource avoids recreating them
/// every frame and enables swapping between normal/glow states.
#[derive(Resource)]
pub struct NodeMaterials {
    /// Normal material for entity nodes.
    pub entity_normal: Handle<StandardMaterial>,
    /// Glow material for selected entity nodes.
    pub entity_glow: Handle<StandardMaterial>,
    /// Normal material for document reference nodes.
    pub docref_normal: Handle<StandardMaterial>,
    /// Glow material for selected document reference nodes.
    pub docref_glow: Handle<StandardMaterial>,
    /// Normal material for start/root node.
    pub start_normal: Handle<StandardMaterial>,
    /// Glow material for selected start/root node.
    pub start_glow: Handle<StandardMaterial>,
    /// Edge materials by relationship type: (normal, glow).
    pub edge_materials: HashMap<String, (Handle<StandardMaterial>, Handle<StandardMaterial>)>,
}
