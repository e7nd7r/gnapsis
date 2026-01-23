//! Visualize subcommand - render graph from JSON file.

use std::path::PathBuf;

use clap::Parser;

use crate::models::QueryGraph;
use crate::visualization::run_visualizer;

/// Visualize a graph from a JSON file.
#[derive(Parser)]
pub struct VisualizeCommand {
    /// Path to JSON file containing QueryGraph data.
    pub input: PathBuf,
}

impl VisualizeCommand {
    /// Run the visualize command.
    pub fn run(self) -> color_eyre::Result<()> {
        let content = std::fs::read_to_string(&self.input)?;

        let query_graph: QueryGraph = serde_json::from_str(&content).map_err(|e| {
            color_eyre::eyre::eyre!(
                "Could not parse {} as QueryGraph: {}",
                self.input.display(),
                e
            )
        })?;

        run_visualizer(query_graph);
        Ok(())
    }
}
