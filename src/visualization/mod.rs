//! 3D Graph Visualization Module
//!
//! Provides force-directed layout and 3D rendering using Bevy.
//! Visualizes QueryGraph results from semantic queries.
//!
//! ## Module Structure
//!
//! - `graph` - Force-directed layout algorithm
//! - `nvim` - Neovim RPC client for file navigation
//! - `components` - ECS components for nodes, edges, labels
//! - `resources` - ECS resources for state (camera, selection, etc.)
//! - `systems` - ECS systems (camera, interaction, physics, UI)
//! - `setup` - Scene initialization
//! - `plugin` - Bevy plugin definition
//! - `constants` - Colors, sizes, and other constants

mod components;
mod constants;
mod graph;
mod nvim;
mod plugin;
mod resources;
mod setup;
mod systems;

pub use graph::{GraphLayout, LayoutNode, NodeType};
pub use nvim::NvimClient;
pub use plugin::VisualizationPlugin;

use crate::models::QueryGraph;
use bevy::prelude::*;
use resources::CameraOrbit;

/// Run the visualizer with a QueryGraph.
///
/// This spawns a Bevy window with the 3D graph visualization.
/// The function blocks until the window is closed.
pub fn run_visualizer(graph: QueryGraph) {
    let mut layout = GraphLayout::from_query_graph(&graph);
    layout.stabilize(500); // Pre-settle before rendering

    // Calculate camera distance based on graph bounds
    let (center, radius) = layout.bounding_sphere();
    let camera_distance = (radius * 2.5).max(10.0); // Ensure minimum distance

    // Try to connect to Neovim
    let nvim_client = NvimClient::try_connect();
    if nvim_client.is_some() {
        eprintln!("Connected to Neovim socket");
    }

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Gnapsis Graph Visualizer".to_string(),
                resolution: (1280.0, 720.0).into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(ClearColor(Color::srgb(0.1, 0.1, 0.12)))
        .insert_resource(CameraOrbit {
            target: center,
            distance: camera_distance,
            ..default()
        })
        .add_plugins(VisualizationPlugin::new(layout, graph, nvim_client))
        .run();
}
