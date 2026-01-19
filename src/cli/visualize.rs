//! Visualize subcommand - render graph from JSON file.

use std::path::PathBuf;

use clap::Parser;

use crate::models::{CompositionGraph, Subgraph};
use crate::visualization::{run_visualizer, VisualizationInput};

/// Visualize a graph from a JSON file.
#[derive(Parser)]
pub struct VisualizeCommand {
    /// Path to JSON file containing subgraph or composition graph data.
    pub input: PathBuf,

    /// Entity ID to highlight as starting node (for subgraph).
    #[arg(short, long)]
    pub start_id: Option<String>,
}

impl VisualizeCommand {
    /// Run the visualize command.
    pub fn run(self) -> color_eyre::Result<()> {
        let content = std::fs::read_to_string(&self.input)?;

        // Try parsing as Subgraph first
        if let Ok(subgraph) = serde_json::from_str::<Subgraph>(&content) {
            let start_id = self.start_id.unwrap_or_else(|| {
                // Find the node with distance 0 as start
                subgraph
                    .nodes
                    .iter()
                    .find_map(|n| match n {
                        crate::models::SubgraphNode::Entity { id, distance, .. }
                            if *distance == 0 =>
                        {
                            Some(id.clone())
                        }
                        _ => None,
                    })
                    .unwrap_or_default()
            });

            run_visualizer(VisualizationInput::Subgraph {
                data: subgraph,
                start_id,
            });
            return Ok(());
        }

        // Try parsing as CompositionGraph
        if let Ok(composition) = serde_json::from_str::<CompositionGraph>(&content) {
            run_visualizer(VisualizationInput::Composition(composition));
            return Ok(());
        }

        Err(color_eyre::eyre::eyre!(
            "Could not parse {} as Subgraph or CompositionGraph",
            self.input.display()
        ))
    }
}
