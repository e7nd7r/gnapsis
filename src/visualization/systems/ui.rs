//! UI systems for labels, info panel, edge hotspots, and glow effects.

use bevy::prelude::*;
use bevy::ui::Node as UiNode;
use std::collections::HashSet;

use crate::visualization::components::{
    EdgeHotspot, GraphEdge, GraphNode, InfoPanelText, NodeLabel,
};
use crate::visualization::constants::{BASE_NODE_RADIUS, MAX_NODE_RADIUS, MIN_NODE_RADIUS};
use crate::visualization::graph::NodeType;
use crate::visualization::resources::{CurrentSelection, GraphLayoutRes, NodeMaterials, Selection};

/// Update label positions by projecting 3D node positions to screen space.
/// Document reference labels are only shown when selected or connected to selection.
pub fn update_labels_system(
    layout: Res<GraphLayoutRes>,
    selection: Res<CurrentSelection>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut label_query: Query<(&mut UiNode, &mut Visibility, &NodeLabel)>,
) {
    let Ok((camera, camera_transform)) = camera_query.get_single() else {
        return;
    };

    // Determine which nodes should show labels based on selection
    let visible_nodes: HashSet<usize> = match &selection.selection {
        Selection::Node(idx) => {
            // Selected node and all connected nodes
            let mut nodes: HashSet<usize> = layout
                .0
                .edges
                .iter()
                .filter_map(|e| {
                    if e.from_idx == *idx {
                        Some(e.to_idx)
                    } else if e.to_idx == *idx {
                        Some(e.from_idx)
                    } else {
                        None
                    }
                })
                .collect();
            nodes.insert(*idx);
            nodes
        }
        Selection::Edge { from_idx, to_idx } => {
            let mut nodes = HashSet::new();
            nodes.insert(*from_idx);
            nodes.insert(*to_idx);
            nodes
        }
        Selection::None => HashSet::new(),
    };

    for (mut node_ui, mut visibility, label) in label_query.iter_mut() {
        if let Some(layout_node) = layout.0.nodes.get(label.node_idx) {
            // Document references only show label when selected or neighbor is selected
            let is_docref = matches!(layout_node.node_type, NodeType::DocumentReference);
            let should_show_label = !is_docref || visible_nodes.contains(&label.node_idx);

            if !should_show_label {
                *visibility = Visibility::Hidden;
                continue;
            }

            // Project 3D position to screen space - offset by node radius so label doesn't overlap
            let radius = (BASE_NODE_RADIUS * layout_node.mass.sqrt())
                .clamp(MIN_NODE_RADIUS, MAX_NODE_RADIUS);
            let label_offset = radius * 1.2 + 0.3; // Just above the node
            let world_pos = layout_node.position + Vec3::Y * label_offset;

            if let Ok(viewport_pos) = camera.world_to_viewport(camera_transform, world_pos) {
                // Check if in front of camera
                let to_node = world_pos - camera_transform.translation();
                let camera_forward = camera_transform.forward();
                let is_in_front = to_node.dot(*camera_forward) > 0.0;

                if is_in_front {
                    *visibility = Visibility::Visible;
                    node_ui.left = Val::Px(viewport_pos.x - 40.0); // Center text roughly
                    node_ui.top = Val::Px(viewport_pos.y - 10.0);
                } else {
                    *visibility = Visibility::Hidden;
                }
            } else {
                *visibility = Visibility::Hidden;
            }
        }
    }
}

/// Update edge hotspot positions by projecting 3D midpoints to screen space.
pub fn update_edge_hotspots_system(
    layout: Res<GraphLayoutRes>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut hotspot_query: Query<(&mut UiNode, &EdgeHotspot)>,
) {
    let Ok((camera, camera_transform)) = camera_query.get_single() else {
        return;
    };

    for (mut node_ui, hotspot) in hotspot_query.iter_mut() {
        let from_pos = layout.0.nodes[hotspot.from_idx].position;
        let to_pos = layout.0.nodes[hotspot.to_idx].position;
        let midpoint = (from_pos + to_pos) / 2.0;

        if let Ok(viewport_pos) = camera.world_to_viewport(camera_transform, midpoint) {
            // Check if in front of camera
            let to_midpoint = midpoint - camera_transform.translation();
            let camera_forward = camera_transform.forward();
            let is_in_front = to_midpoint.dot(*camera_forward) > 0.0;

            if is_in_front {
                // Center the hotspot on the edge midpoint
                node_ui.left = Val::Px(viewport_pos.x - 15.0);
                node_ui.top = Val::Px(viewport_pos.y - 15.0);
            } else {
                // Move offscreen when behind camera
                node_ui.left = Val::Px(-100.0);
                node_ui.top = Val::Px(-100.0);
            }
        } else {
            node_ui.left = Val::Px(-100.0);
            node_ui.top = Val::Px(-100.0);
        }
    }
}

/// Update info panel when a node or edge is selected.
pub fn update_info_panel_system(
    selection: Res<CurrentSelection>,
    layout: Res<GraphLayoutRes>,
    edge_labels: Query<&EdgeHotspot>,
    mut text_query: Query<&mut Text, With<InfoPanelText>>,
) {
    if !selection.is_changed() {
        return;
    }

    let Ok(mut text) = text_query.get_single_mut() else {
        return;
    };

    match &selection.selection {
        Selection::Node(idx) => {
            if let Some(node) = layout.0.nodes.get(*idx) {
                // Count connections for this node
                let connections: Vec<&str> = layout
                    .0
                    .edges
                    .iter()
                    .filter_map(|e| {
                        if e.from_idx == *idx {
                            Some(
                                layout
                                    .0
                                    .nodes
                                    .get(e.to_idx)
                                    .map(|n| n.label.as_str())
                                    .unwrap_or("?"),
                            )
                        } else if e.to_idx == *idx {
                            Some(
                                layout
                                    .0
                                    .nodes
                                    .get(e.from_idx)
                                    .map(|n| n.label.as_str())
                                    .unwrap_or("?"),
                            )
                        } else {
                            None
                        }
                    })
                    .collect();

                let node_type = match node.node_type {
                    NodeType::Entity => "Entity",
                    NodeType::DocumentReference => "Document Ref",
                    NodeType::StartNode => "Start Node",
                };

                **text = format!(
                    "\nName: {}\nID: {}\nType: {}\nMass: {:.2}\nConnections: {}\n\nConnected to:\n{}",
                    node.label,
                    node.id,
                    node_type,
                    node.mass,
                    connections.len(),
                    connections.join("\n")
                );
            }
        }
        Selection::Edge { from_idx, to_idx } => {
            // Find the edge label with note info
            let edge_info = edge_labels
                .iter()
                .find(|e| e.from_idx == *from_idx && e.to_idx == *to_idx);
            let from_name = layout
                .0
                .nodes
                .get(*from_idx)
                .map(|n| n.label.as_str())
                .unwrap_or("?");
            let to_name = layout
                .0
                .nodes
                .get(*to_idx)
                .map(|n| n.label.as_str())
                .unwrap_or("?");

            if let Some(edge) = edge_info {
                let note_text = edge.note.as_deref().unwrap_or("(no note)");
                **text = format!(
                    "\nRelationship\n\nType: {}\nFrom: {}\nTo: {}\n\nNote:\n{}",
                    edge.relationship, from_name, to_name, note_text
                );
            } else {
                // Fallback to layout edge
                if let Some(layout_edge) = layout
                    .0
                    .edges
                    .iter()
                    .find(|e| e.from_idx == *from_idx && e.to_idx == *to_idx)
                {
                    let note_text = layout_edge.note.as_deref().unwrap_or("(no note)");
                    **text = format!(
                        "\nRelationship\n\nType: {}\nFrom: {}\nTo: {}\n\nNote:\n{}",
                        layout_edge.label, from_name, to_name, note_text
                    );
                }
            }
        }
        Selection::None => {
            **text = "Click a node or edge to see details".to_string();
        }
    }
}

/// Update materials to show glow on selected node/edge and its connections.
pub fn update_selection_glow_system(
    selection: Res<CurrentSelection>,
    layout: Res<GraphLayoutRes>,
    node_materials: Res<NodeMaterials>,
    mut node_query: Query<(&GraphNode, &mut MeshMaterial3d<StandardMaterial>), Without<GraphEdge>>,
    mut edge_query: Query<(&GraphEdge, &mut MeshMaterial3d<StandardMaterial>), Without<GraphNode>>,
) {
    // Always update materials to ensure glow state is correct

    // Determine which nodes and edges should glow based on selection
    let (glowing_nodes, glowing_edges): (Vec<usize>, Vec<(usize, usize)>) =
        match &selection.selection {
            Selection::Node(idx) => {
                // Selected node and all connected nodes glow
                let connected: Vec<usize> = layout
                    .0
                    .edges
                    .iter()
                    .filter_map(|e| {
                        if e.from_idx == *idx {
                            Some(e.to_idx)
                        } else if e.to_idx == *idx {
                            Some(e.from_idx)
                        } else {
                            None
                        }
                    })
                    .collect();
                let mut nodes = connected;
                nodes.push(*idx);
                // Edges connected to selected node glow
                let edges: Vec<(usize, usize)> = layout
                    .0
                    .edges
                    .iter()
                    .filter(|e| e.from_idx == *idx || e.to_idx == *idx)
                    .map(|e| (e.from_idx, e.to_idx))
                    .collect();
                (nodes, edges)
            }
            Selection::Edge { from_idx, to_idx } => {
                // Both nodes of selected edge glow, and the edge itself
                (vec![*from_idx, *to_idx], vec![(*from_idx, *to_idx)])
            }
            Selection::None => (vec![], vec![]),
        };

    // Update node materials
    for (graph_node, mut material) in node_query.iter_mut() {
        let should_glow = glowing_nodes.contains(&graph_node.node_idx);

        let new_handle = if should_glow {
            match layout.0.nodes.get(graph_node.node_idx).map(|n| n.node_type) {
                Some(NodeType::Entity) => node_materials.entity_glow.clone(),
                Some(NodeType::DocumentReference) => node_materials.docref_glow.clone(),
                Some(NodeType::StartNode) => node_materials.start_glow.clone(),
                None => continue,
            }
        } else {
            match layout.0.nodes.get(graph_node.node_idx).map(|n| n.node_type) {
                Some(NodeType::Entity) => node_materials.entity_normal.clone(),
                Some(NodeType::DocumentReference) => node_materials.docref_normal.clone(),
                Some(NodeType::StartNode) => node_materials.start_normal.clone(),
                None => continue,
            }
        };
        *material = MeshMaterial3d(new_handle);
    }

    // Update edge materials based on relationship type
    for (edge, mut material) in edge_query.iter_mut() {
        let should_glow = glowing_edges.contains(&(edge.from_idx, edge.to_idx));

        let (normal, glow) = node_materials
            .edge_materials
            .get(&edge.relationship)
            .or_else(|| node_materials.edge_materials.get("_DEFAULT"))
            .cloned()
            .unwrap_or_else(|| {
                // Fallback - shouldn't happen
                node_materials
                    .edge_materials
                    .get("_DEFAULT")
                    .cloned()
                    .unwrap()
            });

        let new_handle = if should_glow { glow } else { normal };
        *material = MeshMaterial3d(new_handle);
    }
}
