//! Graph layout physics system.

use bevy::prelude::*;

use crate::visualization::components::{EdgeArrow, GraphEdge, GraphNode};
use crate::visualization::constants::{BASE_NODE_RADIUS, MAX_NODE_RADIUS, MIN_NODE_RADIUS};
use crate::visualization::resources::GraphLayoutRes;

/// Update layout physics and sync node/edge/arrow positions.
#[allow(clippy::type_complexity)]
pub fn update_layout_system(
    mut layout: ResMut<GraphLayoutRes>,
    mut node_query: Query<(&mut Transform, &GraphNode), (Without<GraphEdge>, Without<EdgeArrow>)>,
    mut edge_query: Query<(&mut Transform, &GraphEdge), (Without<GraphNode>, Without<EdgeArrow>)>,
    mut arrow_query: Query<(&mut Transform, &EdgeArrow), (Without<GraphNode>, Without<GraphEdge>)>,
    time: Res<Time>,
) {
    // Run multiple physics steps per frame for faster settling
    let dt = time.delta_secs();
    for _ in 0..20 {
        layout.0.update_physics(dt);
    }

    // Update node positions
    for (mut transform, graph_node) in node_query.iter_mut() {
        if let Some(node) = layout.0.nodes.iter().find(|n| n.id == graph_node.id) {
            transform.translation = node.position;
        }
    }

    // Update edge positions and rotations
    for (mut transform, edge) in edge_query.iter_mut() {
        let from_pos = layout.0.nodes[edge.from_idx].position;
        let to_pos = layout.0.nodes[edge.to_idx].position;

        let midpoint = (from_pos + to_pos) / 2.0;
        let direction = to_pos - from_pos;
        let length = direction.length();

        if length > 0.01 {
            let rotation = Quat::from_rotation_arc(Vec3::Y, direction.normalize());
            transform.translation = midpoint;
            transform.rotation = rotation;
            transform.scale = Vec3::new(1.0, length, 1.0);
        }
    }

    // Update arrowhead positions at target end of each edge
    for (mut transform, arrow) in arrow_query.iter_mut() {
        let from_pos = layout.0.nodes[arrow.from_idx].position;
        let to_pos = layout.0.nodes[arrow.to_idx].position;

        let direction = to_pos - from_pos;
        let length = direction.length();

        if length > 0.01 {
            let dir_norm = direction.normalize();
            let target_node = &layout.0.nodes[arrow.to_idx];
            let target_radius = (BASE_NODE_RADIUS * target_node.mass.sqrt())
                .clamp(MIN_NODE_RADIUS, MAX_NODE_RADIUS);

            transform.translation = to_pos - dir_norm * (target_radius + 0.2);
            transform.rotation = Quat::from_rotation_arc(Vec3::Y, dir_norm);
        }
    }
}
