//! Node dragging and selection systems.

use bevy::input::mouse::MouseMotion;
use bevy::prelude::*;
use bevy::ui::Node as UiNode;

use crate::visualization::components::{EdgeHotspot, GraphNode};
use crate::visualization::resources::{CurrentSelection, DragState, GraphLayoutRes, Selection};

/// Drag nodes with left-click. Shift+drag to push in depth. Click to select.
#[allow(clippy::too_many_arguments)]
pub fn drag_node_system(
    mut drag_state: ResMut<DragState>,
    mut layout: ResMut<GraphLayoutRes>,
    mut selection: ResMut<CurrentSelection>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    node_query: Query<(Entity, &Transform, &GraphNode)>,
    edge_hotspot_query: Query<(&UiNode, &EdgeHotspot)>,
    mut mouse_motion: EventReader<MouseMotion>,
) {
    let Ok(window) = windows.get_single() else {
        return;
    };
    let Ok((camera, camera_transform)) = camera_query.get_single() else {
        return;
    };

    // Start drag on left click
    if mouse_button.just_pressed(MouseButton::Left) {
        if let Some(cursor_pos) = window.cursor_position() {
            // Cast ray from cursor
            if let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_pos) {
                // Find closest node hit by ray
                let mut closest: Option<(Entity, usize, f32, Vec3)> = None;

                for (entity, transform, graph_node) in node_query.iter() {
                    let node_pos = transform.translation;
                    let to_node = node_pos - ray.origin;
                    let t = to_node.dot(*ray.direction);

                    if t > 0.0 {
                        let closest_point = ray.origin + *ray.direction * t;
                        let distance = (closest_point - node_pos).length();

                        // Hit radius (slightly larger than visual for easier selection)
                        let hit_radius = graph_node.radius * 1.8;

                        if distance < hit_radius
                            && (closest.is_none() || t < closest.unwrap().2)
                        {
                            closest = Some((entity, graph_node.node_idx, t, node_pos));
                        }
                    }
                }

                if let Some((entity, node_idx, depth, node_pos)) = closest {
                    // Calculate the point where the ray intersects the plane at node depth
                    let hit_point = ray.origin + *ray.direction * depth;
                    let grab_offset = node_pos - hit_point;

                    drag_state.dragging = Some(entity);
                    drag_state.node_idx = Some(node_idx);
                    drag_state.drag_depth = depth;
                    drag_state.total_movement = 0.0;
                    drag_state.grab_offset = grab_offset;
                    // Reset stability so physics responds to drag
                    layout.0.stable = false;
                }
            }
        }
    }

    // Continue drag - project cursor to 3D plane at node depth
    if mouse_button.pressed(MouseButton::Left) && drag_state.dragging.is_some() {
        // Track mouse motion for click vs drag detection
        let mut total_delta = Vec2::ZERO;
        for ev in mouse_motion.read() {
            total_delta += ev.delta;
        }
        drag_state.total_movement += total_delta.length();

        if let Some(node_idx) = drag_state.node_idx {
            if let Some(cursor_pos) = window.cursor_position() {
                if let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_pos) {
                    // Shift + drag = push in depth (toward/away from camera)
                    if keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight)
                    {
                        // Adjust depth based on vertical mouse movement
                        let depth_change = total_delta.y * 0.05;
                        drag_state.drag_depth = (drag_state.drag_depth + depth_change).max(1.0);
                    }

                    // Project cursor ray to the plane at drag_depth
                    // The plane is perpendicular to camera forward at distance drag_depth
                    let camera_pos = camera_transform.translation();
                    let camera_forward = camera_transform.forward();

                    // Plane equation: point where ray.origin + t * ray.direction
                    // reaches distance drag_depth from camera along forward
                    // We use a plane perpendicular to camera forward
                    let plane_point = camera_pos + *camera_forward * drag_state.drag_depth;
                    let plane_normal = *camera_forward;

                    // Ray-plane intersection: t = (plane_point - ray.origin) . normal / (ray.direction . normal)
                    let denom = ray.direction.dot(plane_normal);
                    if denom.abs() > 0.0001 {
                        let t = (plane_point - ray.origin).dot(plane_normal) / denom;
                        if t > 0.0 {
                            let new_pos = ray.origin + *ray.direction * t + drag_state.grab_offset;
                            layout.0.nodes[node_idx].position = new_pos;
                            layout.0.nodes[node_idx].velocity = Vec3::ZERO;
                        }
                    }

                    // Keep physics running while dragging
                    layout.0.stable = false;
                }
            }
        }
    }

    // End drag on release
    if mouse_button.just_released(MouseButton::Left) {
        // If minimal movement, treat as click -> select node or check edge hotspots
        if drag_state.total_movement < 5.0 {
            if let Some(node_idx) = drag_state.node_idx {
                selection.selection = Selection::Node(node_idx);
            } else if let Some(cursor_pos) = window.cursor_position() {
                // Check if clicked on an edge hotspot (invisible click area at edge midpoint)
                let mut clicked_edge = false;
                for (node_ui, hotspot) in edge_hotspot_query.iter() {
                    let left = match node_ui.left {
                        Val::Px(v) => v,
                        _ => continue,
                    };
                    let top = match node_ui.top {
                        Val::Px(v) => v,
                        _ => continue,
                    };
                    // Hotspot is 30x30 pixels centered on edge midpoint
                    let hotspot_size = 30.0;
                    if cursor_pos.x >= left
                        && cursor_pos.x <= left + hotspot_size
                        && cursor_pos.y >= top
                        && cursor_pos.y <= top + hotspot_size
                    {
                        selection.selection = Selection::Edge {
                            from_idx: hotspot.from_idx,
                            to_idx: hotspot.to_idx,
                        };
                        clicked_edge = true;
                        break;
                    }
                }
                // Clicked on empty space - clear selection
                if !clicked_edge {
                    selection.selection = Selection::None;
                }
            }
        }
        drag_state.dragging = None;
        drag_state.node_idx = None;
    }
}
