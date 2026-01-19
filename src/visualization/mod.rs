//! 3D Graph Visualization Module
//!
//! Provides force-directed layout and 3D rendering using Bevy.
//! Works directly with `Subgraph` and `CompositionGraph` models.
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

use crate::models::{CompositionGraph, Subgraph};
use bevy::prelude::*;

/// Input mode for visualization.
pub enum VisualizationInput {
    /// Visualize a subgraph centered on a starting entity.
    Subgraph { data: Subgraph, start_id: String },
    /// Visualize a composition graph (ancestors/descendants).
    Composition(CompositionGraph),
}

/// Run the visualizer with the given graph data.
///
/// This spawns a Bevy window with the 3D graph visualization.
/// The function blocks until the window is closed.
pub fn run_visualizer(input: VisualizationInput) {
    // Extract subgraph data and create layout
    let (layout, subgraph_data) = match &input {
        VisualizationInput::Subgraph { data, start_id } => {
            let mut layout = GraphLayout::from_subgraph(data, start_id);
            layout.stabilize(500); // Pre-settle before rendering
            (layout, Some(data.clone()))
        }
        VisualizationInput::Composition(data) => {
            let mut layout = GraphLayout::from_composition(data);
            layout.stabilize(500); // Pre-settle before rendering
            (layout, None)
        }
    };

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
        .add_plugins(VisualizationPlugin::new(layout, subgraph_data, nvim_client))
        .run();
}
