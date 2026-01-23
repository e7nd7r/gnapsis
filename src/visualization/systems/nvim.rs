//! Neovim integration for opening DocumentReferences.

use bevy::prelude::*;

use crate::models::QueryGraphNode;
use crate::visualization::graph::NodeType;
use crate::visualization::nvim::DocRefInfo;
use crate::visualization::nvim::NvimVisualization;
use crate::visualization::resources::{CurrentSelection, GraphLayoutRes, NvimClientRes, Selection};

/// Show document references in Neovim picker when a node is selected.
///
/// - If a Reference is selected: opens that file location directly
/// - If an Entity is selected: shows all connected References in the picker
pub fn nvim_integration_system(
    selection: Res<CurrentSelection>,
    layout: Res<GraphLayoutRes>,
    nvim_client: Res<NvimClientRes>,
    query_graph: Option<Res<QueryGraphRes>>,
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

    // Get the query graph data (needed for reference lookups)
    let graph = match &query_graph {
        Some(g) => &g.0,
        None => return,
    };

    // Collect document references based on selection type
    let (refs, title): (Vec<DocRefInfo>, String) = match layout_node.node_type {
        NodeType::DocumentReference => {
            // Selected a Reference directly - find it in the graph
            let doc_ref = graph.nodes.iter().find_map(|node| match node {
                QueryGraphNode::Reference {
                    id,
                    document_path,
                    start_line,
                    end_line,
                    description,
                    ..
                } if id == &layout_node.id => Some(DocRefInfo {
                    path: document_path.clone(),
                    start_line: *start_line,
                    end_line: *end_line,
                    description: description.clone(),
                }),
                _ => None,
            });

            match doc_ref {
                Some(r) => (vec![r], "Document Reference".to_string()),
                None => return,
            }
        }

        NodeType::Entity | NodeType::StartNode => {
            // Selected an Entity - find all connected References
            let connected_ref_ids: Vec<&str> = layout
                .0
                .edges
                .iter()
                .filter_map(|e| {
                    // Check if this edge connects to our selected node
                    let other_idx = if e.from_idx == node_idx {
                        Some(e.to_idx)
                    } else if e.to_idx == node_idx {
                        Some(e.from_idx)
                    } else {
                        None
                    }?;

                    // Check if the other node is a DocumentReference
                    let other_node = layout.0.nodes.get(other_idx)?;
                    if matches!(other_node.node_type, NodeType::DocumentReference) {
                        Some(other_node.id.as_str())
                    } else {
                        None
                    }
                })
                .collect();

            if connected_ref_ids.is_empty() {
                return;
            }

            // Get full info for each Reference
            let refs: Vec<DocRefInfo> = graph
                .nodes
                .iter()
                .filter_map(|node| match node {
                    QueryGraphNode::Reference {
                        id,
                        document_path,
                        start_line,
                        end_line,
                        description,
                        ..
                    } if connected_ref_ids.contains(&id.as_str()) => Some(DocRefInfo {
                        path: document_path.clone(),
                        start_line: *start_line,
                        end_line: *end_line,
                        description: description.clone(),
                    }),
                    _ => None,
                })
                .collect();

            if refs.is_empty() {
                return;
            }

            let title = format!("References for {}", layout_node.label);
            (refs, title)
        }
    };

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

use crate::models::QueryGraph;

/// Resource to hold the QueryGraph data for reference lookups.
#[derive(Resource)]
pub struct QueryGraphRes(pub QueryGraph);
