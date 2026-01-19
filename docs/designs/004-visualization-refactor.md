# Design: Visualization Module Refactoring

## Overview

The 3D graph visualization module has grown organically to ~1800 lines across 4 files, with `renderer.rs` alone at 1244 lines. This design proposes a refactoring based on Bevy best practices to improve maintainability, testability, and clarity.

## Current State

```
visualization/
├── mod.rs          (12 lines)   - Module exports
├── graph.rs        (348 lines)  - Force-directed layout algorithm
├── nvim.rs         (174 lines)  - Neovim RPC client
└── renderer.rs     (1244 lines) - Everything else (components, resources, systems, setup)
```

### Problems with Current Structure

1. **Monolithic renderer.rs** - Contains 15+ components, 8+ resources, 10+ systems, and setup logic
2. **No clear separation of concerns** - Camera, selection, physics, UI all mixed together
3. **Difficult to test** - Systems are tightly coupled
4. **Hard to navigate** - Finding specific functionality requires scrolling through 1200+ lines

## Proposed Structure

```
visualization/
├── mod.rs              # Plugin composition and public exports
├── plugin.rs           # VisualizationPlugin definition
├── components.rs       # All ECS components
├── resources.rs        # All ECS resources
├── constants.rs        # Colors, sizes, physics constants
├── graph.rs            # Force-directed layout (unchanged)
├── nvim.rs             # Neovim client (unchanged)
├── setup.rs            # Scene setup and material creation
└── systems/
    ├── mod.rs          # System exports and ordering
    ├── camera.rs       # Camera orbit, pan, zoom
    ├── interaction.rs  # Drag nodes, click detection, selection
    ├── physics.rs      # Layout physics updates
    ├── ui.rs           # Labels, info panel, edge hotspots
    └── nvim.rs         # Neovim integration on selection
```

## Detailed Design

### 1. `constants.rs`

Extract all constants to a dedicated file:

```rust
//! Visual and physics constants for the graph visualization.

use bevy::prelude::*;

// Node colors by type
pub const COLOR_ENTITY: Color = Color::srgb(0.29, 0.56, 0.85);
pub const COLOR_DOCREF: Color = Color::srgb(0.36, 0.72, 0.36);
pub const COLOR_START: Color = Color::srgb(1.0, 0.84, 0.0);

// Edge colors by relationship type
pub const COLOR_BELONGS_TO: Color = Color::srgb(0.6, 0.4, 0.8);
pub const COLOR_CALLS: Color = Color::srgb(1.0, 0.5, 0.3);
pub const COLOR_HAS_REFERENCE: Color = Color::srgb(0.3, 0.7, 0.9);
// ... etc

// Node sizing
pub const BASE_NODE_RADIUS: f32 = 0.2;
pub const MIN_NODE_RADIUS: f32 = 0.15;
pub const MAX_NODE_RADIUS: f32 = 0.8;
```

### 2. `components.rs`

All ECS components in one place:

```rust
//! ECS components for graph visualization.

use bevy::prelude::*;

/// Marker for graph nodes with selection data.
#[derive(Component)]
pub struct GraphNode {
    pub id: String,
    pub node_idx: usize,
    pub radius: f32,
}

/// Marker for edge lines.
#[derive(Component)]
pub struct GraphEdge {
    pub from_idx: usize,
    pub to_idx: usize,
    pub relationship: String,
}

/// Label that follows a node.
#[derive(Component)]
pub struct NodeLabel {
    pub node_idx: usize,
}

/// Invisible hotspot for edge click detection.
#[derive(Component)]
pub struct EdgeHotspot {
    pub from_idx: usize,
    pub to_idx: usize,
    pub relationship: String,
    pub note: Option<String>,
}

/// Marker for the info panel.
#[derive(Component)]
pub struct InfoPanel;

/// Marker for info panel text content.
#[derive(Component)]
pub struct InfoPanelText;
```

### 3. `resources.rs`

All ECS resources:

```rust
//! ECS resources for graph visualization state.

use bevy::prelude::*;
use std::sync::Mutex;
use std::collections::HashMap;

use super::graph::GraphLayout;
use super::nvim::NvimClient;
use crate::models::Subgraph;

/// Camera orbit state.
#[derive(Resource, Default)]
pub struct CameraOrbit {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub target: Vec3,
}

/// State for dragging nodes.
#[derive(Resource, Default)]
pub struct DragState {
    pub dragging: Option<Entity>,
    pub node_idx: Option<usize>,
    pub drag_depth: f32,
    pub total_movement: f32,
    pub grab_offset: Vec3,
}

/// What is currently selected.
#[derive(Clone, Default)]
pub enum Selection {
    #[default]
    None,
    Node(usize),
    Edge { from_idx: usize, to_idx: usize },
}

/// Currently selected element.
#[derive(Resource, Default)]
pub struct CurrentSelection {
    pub selection: Selection,
}

/// Graph layout wrapper.
#[derive(Resource)]
pub struct GraphLayoutRes(pub GraphLayout);

/// Neovim client (optional).
#[derive(Resource)]
pub struct NvimClientRes(pub Mutex<Option<NvimClient>>);

/// Original subgraph data for DocumentReference lookups.
#[derive(Resource, Default)]
pub struct SubgraphDataRes(pub Option<Subgraph>);

/// Pre-created materials for nodes and edges.
#[derive(Resource)]
pub struct NodeMaterials {
    pub entity_normal: Handle<StandardMaterial>,
    pub entity_glow: Handle<StandardMaterial>,
    pub docref_normal: Handle<StandardMaterial>,
    pub docref_glow: Handle<StandardMaterial>,
    pub start_normal: Handle<StandardMaterial>,
    pub start_glow: Handle<StandardMaterial>,
    pub edge_materials: HashMap<String, (Handle<StandardMaterial>, Handle<StandardMaterial>)>,
}
```

### 4. `systems/camera.rs`

Camera control systems:

```rust
//! Camera orbit, pan, and zoom systems.

use bevy::prelude::*;
use bevy::input::mouse::{MouseMotion, MouseWheel};

use crate::visualization::resources::CameraOrbit;

/// Handle camera orbit with right-click drag.
pub fn camera_orbit_system(
    mut orbit: ResMut<CameraOrbit>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut mouse_motion: EventReader<MouseMotion>,
    mut scroll: EventReader<MouseWheel>,
    mut camera_query: Query<&mut Transform, With<Camera3d>>,
) {
    // ... implementation
}

/// Calculate camera position from orbit state.
pub fn calculate_camera_position(orbit: &CameraOrbit) -> Vec3 {
    // ... implementation
}
```

### 5. `systems/interaction.rs`

Node dragging and selection:

```rust
//! Node dragging and selection systems.

use bevy::prelude::*;

use crate::visualization::components::*;
use crate::visualization::resources::*;

/// Handle node dragging and click selection.
pub fn drag_node_system(
    mut drag_state: ResMut<DragState>,
    mut layout: ResMut<GraphLayoutRes>,
    mut selection: ResMut<CurrentSelection>,
    // ... params
) {
    // ... implementation
}
```

### 6. `systems/physics.rs`

Layout physics updates:

```rust
//! Graph layout physics system.

use bevy::prelude::*;

use crate::visualization::components::*;
use crate::visualization::resources::*;

/// Update layout physics and sync node/edge positions.
pub fn update_layout_system(
    mut layout: ResMut<GraphLayoutRes>,
    mut node_query: Query<(&mut Transform, &GraphNode), Without<GraphEdge>>,
    mut edge_query: Query<(&mut Transform, &GraphEdge), Without<GraphNode>>,
    time: Res<Time>,
) {
    // ... implementation
}
```

### 7. `systems/ui.rs`

UI-related systems:

```rust
//! UI systems for labels, info panel, and glow effects.

use bevy::prelude::*;

use crate::visualization::components::*;
use crate::visualization::resources::*;

/// Update label positions in screen space.
pub fn update_labels_system(...) { ... }

/// Update edge hotspot positions.
pub fn update_edge_hotspots_system(...) { ... }

/// Update info panel content.
pub fn update_info_panel_system(...) { ... }

/// Update material glow based on selection.
pub fn update_selection_glow_system(...) { ... }
```

### 8. `systems/nvim.rs`

Neovim integration:

```rust
//! Neovim integration for opening DocumentReferences.

use bevy::prelude::*;

use crate::visualization::components::*;
use crate::visualization::resources::*;
use crate::models::SubgraphNode;

/// Open DocumentReference in Neovim when selected.
pub fn nvim_integration_system(
    selection: Res<CurrentSelection>,
    layout: Res<GraphLayoutRes>,
    subgraph_data: Res<SubgraphDataRes>,
    nvim_client: Res<NvimClientRes>,
) {
    // ... implementation
}
```

### 9. `plugin.rs`

Main plugin definition:

```rust
//! Visualization plugin for Bevy.

use bevy::prelude::*;

use super::components::*;
use super::resources::*;
use super::systems;
use super::setup::setup_scene;

/// Plugin that adds 3D graph visualization.
pub struct VisualizationPlugin {
    pub layout: GraphLayout,
    pub subgraph_data: Option<Subgraph>,
    pub nvim_client: Option<NvimClient>,
}

impl Plugin for VisualizationPlugin {
    fn build(&self, app: &mut App) {
        app
            .insert_resource(CameraOrbit::default())
            .insert_resource(DragState::default())
            .insert_resource(CurrentSelection::default())
            .insert_resource(GraphLayoutRes(self.layout.clone()))
            .insert_resource(SubgraphDataRes(self.subgraph_data.clone()))
            .insert_resource(NvimClientRes(Mutex::new(self.nvim_client.take())))
            .add_systems(Startup, setup_scene)
            .add_systems(Update, (
                systems::camera::camera_orbit_system,
                systems::interaction::drag_node_system,
                systems::physics::update_layout_system,
                systems::ui::update_labels_system,
                systems::ui::update_edge_hotspots_system,
                systems::ui::update_info_panel_system,
                systems::ui::update_selection_glow_system,
                systems::nvim::nvim_integration_system,
            ));
    }
}
```

### 10. `mod.rs`

Clean public interface:

```rust
//! 3D Graph Visualization Module
//!
//! Provides force-directed layout and 3D rendering using Bevy.

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

/// Input mode for visualization.
pub enum VisualizationInput {
    Subgraph { data: Subgraph, start_id: String },
    Composition(CompositionGraph),
}

/// Run the visualizer with the given graph data.
pub fn run_visualizer(input: VisualizationInput) {
    let (layout, subgraph_data) = match &input {
        VisualizationInput::Subgraph { data, start_id } => {
            let mut layout = GraphLayout::from_subgraph(data, start_id);
            layout.stabilize(500);
            (layout, Some(data.clone()))
        }
        VisualizationInput::Composition(data) => {
            let mut layout = GraphLayout::from_composition(data);
            layout.stabilize(500);
            (layout, None)
        }
    };

    let nvim_client = NvimClient::try_connect();

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin { ... }))
        .add_plugins(VisualizationPlugin {
            layout,
            subgraph_data,
            nvim_client,
        })
        .run();
}
```

## Migration Plan

### Phase 1: Extract Constants and Types
1. Create `constants.rs` with all color and size constants
2. Create `components.rs` with all component structs
3. Create `resources.rs` with all resource structs
4. Update `renderer.rs` to import from new modules

### Phase 2: Extract Systems
1. Create `systems/` directory
2. Move camera systems to `systems/camera.rs`
3. Move interaction systems to `systems/interaction.rs`
4. Move physics system to `systems/physics.rs`
5. Move UI systems to `systems/ui.rs`
6. Move nvim system to `systems/nvim.rs`

### Phase 3: Extract Setup
1. Move scene setup to `setup.rs`
2. Move material creation to `setup.rs` or `materials.rs`

### Phase 4: Create Plugin
1. Create `plugin.rs` with `VisualizationPlugin`
2. Update `mod.rs` with clean public interface
3. Delete old `renderer.rs`

### Phase 5: Cleanup
1. Run `cargo fmt`
2. Run `cargo clippy` and fix warnings
3. Test all functionality

## Benefits

1. **Maintainability** - Each file has a clear, single purpose
2. **Discoverability** - Easy to find specific functionality
3. **Testability** - Systems can be unit tested in isolation
4. **Extensibility** - New features can be added as new systems/plugins
5. **Team collaboration** - Multiple people can work on different systems

## References

- [Bevy Code Organization | Tainted Coders](https://taintedcoders.com/bevy/code-organization)
- [Bevy Best Practices GitHub](https://github.com/tbillington/bevy_best_practices)
- [Plugins - Unofficial Bevy Cheat Book](https://bevy-cheatbook.github.io/programming/plugins.html)
