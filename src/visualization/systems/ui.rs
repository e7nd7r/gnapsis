//! UI systems for labels, info panel, edge hotspots, and glow effects.

use bevy::prelude::*;
use bevy::ui::Node as UiNode;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

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

    // Compute 3-hop neighborhood for selection
    let selection_nodes: HashSet<usize> = match &selection.selection {
        Selection::Node(idx) => {
            let (nodes, _) = layout.0.collect_n_hop_neighborhood(*idx, 2);
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

    let has_selection = !matches!(selection.selection, Selection::None);

    for (mut node_ui, mut visibility, label) in label_query.iter_mut() {
        if let Some(layout_node) = layout.0.nodes.get(label.node_idx) {
            let should_show_label = if has_selection {
                // Selection active: only show labels in the 3-hop neighborhood
                selection_nodes.contains(&label.node_idx)
            } else {
                // No selection: show labels for Domain, Feature, Namespace scopes
                matches!(
                    layout_node.scope.as_deref(),
                    Some("Domain" | "Feature" | "Namespace")
                )
            };

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
///
/// Shows entity info with references nested under each connected entity.
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
                let mut lines = Vec::new();

                // Header
                lines.push(format!("Name: {}", node.label));
                if let Some(scope) = &node.scope {
                    lines.push(format!("Scope: {}", scope));
                }

                // Selected node's own references
                if let Some(refs) = layout.0.entity_references.get(&node.id) {
                    lines.push(String::new());
                    lines.push("References:".to_string());
                    for r in refs {
                        lines.push(format!("  {}:{}-{}", r.path, r.start_line, r.end_line));
                        lines.push(format!("    {}", r.description));
                    }
                }

                // Build 2-hop neighborhood with hop tracking
                let connections = collect_connections_with_hops(&layout.0, *idx, 2);

                if !connections.is_empty() {
                    lines.push(String::new());
                    lines.push("Connections:".to_string());

                    // Group by relationship type (sorted)
                    let mut grouped: BTreeMap<String, Vec<(usize, Option<usize>)>> =
                        BTreeMap::new();
                    for (neighbor_idx, rel_type, via_idx) in &connections {
                        grouped
                            .entry(rel_type.clone())
                            .or_default()
                            .push((*neighbor_idx, *via_idx));
                    }

                    for (rel_type, neighbors) in &grouped {
                        lines.push(format!("  {}:", rel_type));
                        for (neighbor_idx, via_idx) in neighbors {
                            let neighbor = &layout.0.nodes[*neighbor_idx];
                            let via_str = via_idx
                                .and_then(|v| layout.0.nodes.get(v))
                                .map(|n| format!(" (via {})", n.label))
                                .unwrap_or_default();
                            lines.push(format!("    {}{}", neighbor.label, via_str));

                            // Show references for this connected entity
                            if let Some(refs) = layout.0.entity_references.get(&neighbor.id) {
                                for r in refs {
                                    lines.push(format!(
                                        "      {}:{}-{}",
                                        r.path, r.start_line, r.end_line
                                    ));
                                }
                            }
                        }
                    }
                }

                **text = lines.join("\n");
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

/// Collect connections from `start` up to `max_hops`, tracking intermediary nodes.
///
/// Returns Vec of (neighbor_idx, relationship_type, optional_via_idx).
/// 1-hop neighbors have via_idx = None, 2-hop neighbors have via_idx = Some(intermediary).
fn collect_connections_with_hops(
    layout: &crate::visualization::graph::GraphLayout,
    start: usize,
    max_hops: usize,
) -> Vec<(usize, String, Option<usize>)> {
    // BFS tracking distance and parent for each visited node
    let mut distances: HashMap<usize, usize> = HashMap::new();
    let mut parent: HashMap<usize, (usize, String)> = HashMap::new(); // node -> (came_from, edge_label)
    let mut queue = VecDeque::new();

    distances.insert(start, 0);
    queue.push_back(start);

    while let Some(current) = queue.pop_front() {
        let depth = distances[&current];
        if depth >= max_hops {
            continue;
        }

        for edge in &layout.edges {
            // BELONGS_TO: only traverse parent → child direction
            let neighbor = if edge.label == "BELONGS_TO" {
                if edge.to_idx == current {
                    Some((edge.from_idx, &edge.label))
                } else {
                    None
                }
            } else if edge.from_idx == current {
                Some((edge.to_idx, &edge.label))
            } else if edge.to_idx == current {
                Some((edge.from_idx, &edge.label))
            } else {
                None
            };

            if let Some((n, label)) = neighbor {
                if let std::collections::hash_map::Entry::Vacant(e) = distances.entry(n) {
                    e.insert(depth + 1);
                    parent.insert(n, (current, label.clone()));
                    queue.push_back(n);
                }
            }
        }
    }

    // Build result: for each non-start node, determine relationship and via
    let mut result = Vec::new();
    for (&node_idx, &dist) in &distances {
        if node_idx == start {
            continue;
        }
        // Walk back to find the relationship type and intermediary
        let (via_idx, rel_type) = if dist == 1 {
            let (_, ref label) = parent[&node_idx];
            (None, label.clone())
        } else if dist == 2 {
            // 2-hop: node_idx -> intermediary -> start
            let (intermediary, _) = parent[&node_idx];
            let (_, ref label) = parent[&intermediary];
            (Some(intermediary), label.clone())
        } else {
            continue; // skip deeper hops
        };
        result.push((node_idx, rel_type, via_idx));
    }

    result
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

    // Determine which nodes and edges should glow based on selection (3-hop neighborhood)
    let (glowing_nodes, glowing_edges): (HashSet<usize>, HashSet<(usize, usize)>) =
        match &selection.selection {
            Selection::Node(idx) => layout.0.collect_n_hop_neighborhood(*idx, 2),
            Selection::Edge { from_idx, to_idx } => {
                let mut nodes = HashSet::new();
                nodes.insert(*from_idx);
                nodes.insert(*to_idx);
                let mut edges = HashSet::new();
                edges.insert((*from_idx, *to_idx));
                (nodes, edges)
            }
            Selection::None => (HashSet::new(), HashSet::new()),
        };

    // Update node materials
    for (graph_node, mut material) in node_query.iter_mut() {
        let should_glow = glowing_nodes.contains(&graph_node.node_idx);

        let layout_node = match layout.0.nodes.get(graph_node.node_idx) {
            Some(n) => n,
            None => continue,
        };

        let new_handle = match layout_node.node_type {
            NodeType::StartNode => {
                if should_glow {
                    node_materials.start_glow.clone()
                } else {
                    node_materials.start_normal.clone()
                }
            }
            NodeType::Entity => {
                let scope_key = layout_node.scope.as_deref().unwrap_or("_DEFAULT");
                let (normal, glow) = node_materials
                    .scope_materials
                    .get(scope_key)
                    .or_else(|| node_materials.scope_materials.get("_DEFAULT"))
                    .cloned()
                    .unwrap();
                if should_glow {
                    glow
                } else {
                    normal
                }
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
