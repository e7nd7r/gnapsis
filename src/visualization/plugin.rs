//! Visualization plugin for Bevy.

use bevy::prelude::*;
use std::sync::Mutex;

use super::graph::GraphLayout;
use super::nvim::NvimClient;
use super::resources::{CameraOrbit, CurrentSelection, DragState, GraphLayoutRes, NvimClientRes};
use super::setup::setup_scene;
use super::systems;
use crate::models::QueryGraph;

/// Plugin that adds 3D graph visualization.
///
/// The `nvim_client` field uses `Mutex<Option<...>>` to allow moving
/// ownership into the resource during `build()` (which takes `&self`).
pub struct VisualizationPlugin {
    /// Pre-computed graph layout.
    pub layout: GraphLayout,
    /// Query graph data for reference lookups.
    pub query_graph: QueryGraph,
    /// Neovim client for file navigation (taken during build).
    pub nvim_client: Mutex<Option<NvimClient>>,
}

impl VisualizationPlugin {
    /// Create a new visualization plugin.
    pub fn new(
        layout: GraphLayout,
        query_graph: QueryGraph,
        nvim_client: Option<NvimClient>,
    ) -> Self {
        Self {
            layout,
            query_graph,
            nvim_client: Mutex::new(nvim_client),
        }
    }
}

impl Plugin for VisualizationPlugin {
    fn build(&self, app: &mut App) {
        // Take ownership of nvim_client (moves it out, leaves None)
        let nvim_client = self.nvim_client.lock().unwrap().take();

        // Only insert CameraOrbit if not already set (allows pre-configuration)
        app.init_resource::<CameraOrbit>()
            .insert_resource(DragState::default())
            .insert_resource(CurrentSelection::default())
            .insert_resource(GraphLayoutRes(self.layout.clone()))
            .insert_resource(systems::QueryGraphRes(self.query_graph.clone()))
            .insert_resource(NvimClientRes(Mutex::new(nvim_client)))
            .add_systems(Startup, setup_scene)
            .add_systems(
                Update,
                (
                    systems::camera_orbit_system,
                    systems::drag_node_system,
                    systems::update_layout_system,
                    systems::update_labels_system,
                    systems::update_edge_hotspots_system,
                    systems::update_info_panel_system,
                    systems::update_selection_glow_system,
                    systems::nvim_integration_system,
                ),
            );
    }
}
