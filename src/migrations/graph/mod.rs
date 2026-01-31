//! Graph-level migrations (per-graph, run once per graph).

mod m001_seed_data;
mod m002_ontology_v2;
mod m003_ontology_v2_data;

pub use m001_seed_data::M001SeedData;
pub use m002_ontology_v2::M002OntologyV2;
pub use m003_ontology_v2_data::M003OntologyV2Data;

use crate::migrations::traits::{GraphMigration, Register};

/// Create the graph migrations register for a given graph.
pub fn create_register(graph_name: &str) -> Register<dyn GraphMigration> {
    Register::<dyn GraphMigration>::new()
        .register(M001SeedData::new(graph_name))
        .register(M002OntologyV2::new(graph_name))
        .register(M003OntologyV2Data::new(graph_name))
}
