//! Neovim integration for opening document references.

use bevy::prelude::*;

use crate::visualization::nvim::DocRefInfo;
use crate::visualization::nvim::NvimVisualization;
use crate::visualization::resources::{CurrentSelection, GraphLayoutRes, NvimClientRes, Selection};

/// Show document references in Neovim picker when a node is selected.
///
/// Looks up references from `entity_references` in the layout.
pub fn nvim_integration_system(
    selection: Res<CurrentSelection>,
    layout: Res<GraphLayoutRes>,
    nvim_client: Res<NvimClientRes>,
) {
    // Only act when selection changes
    if !selection.is_changed() {
        return;
    }

    // Get the selected node index
    let node_idx = match &selection.selection {
        Selection::Node(idx) => *idx,
        _ => return,
    };

    let layout_node = match layout.0.nodes.get(node_idx) {
        Some(node) => node,
        None => return,
    };

    // Collect references from all entities in the 2-hop subgraph
    let (neighborhood, _) = layout.0.collect_n_hop_neighborhood(node_idx, 2);

    let mut refs: Vec<DocRefInfo> = Vec::new();
    for &idx in &neighborhood {
        if let Some(node) = layout.0.nodes.get(idx) {
            if let Some(ref_infos) = layout.0.entity_references.get(&node.id) {
                for r in ref_infos {
                    refs.push(DocRefInfo {
                        path: r.path.clone(),
                        start_line: r.start_line,
                        end_line: r.end_line,
                        description: format!("[{}] {}", node.label, r.description),
                    });
                }
            }
        }
    }

    if refs.is_empty() {
        return;
    }

    let title = format!("References for {} (subgraph)", layout_node.label);

    // Show references panel in Neovim
    let mut client_guard = match nvim_client.0.lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };

    if let Some(client) = client_guard.as_mut() {
        if let Err(e) = client.show_references_picker(&refs, &title) {
            let _ = client.command(&format!("echoerr 'Gnapsis: {}'", e.replace('\'', "''")));
        }
    }
}
