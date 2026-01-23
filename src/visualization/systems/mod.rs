//! ECS systems for graph visualization.
//!
//! Systems are functions that operate on components and resources each frame.

pub mod camera;
pub mod interaction;
pub mod nvim;
pub mod physics;
pub mod ui;

pub use camera::camera_orbit_system;
pub use interaction::drag_node_system;
pub use nvim::{nvim_integration_system, QueryGraphRes};
pub use physics::update_layout_system;
pub use ui::{
    update_edge_hotspots_system, update_info_panel_system, update_labels_system,
    update_selection_glow_system,
};
