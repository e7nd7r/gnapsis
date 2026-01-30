//! Schema migrations for PostgreSQL + Apache AGE with version tracking.
//!
//! This module implements a two-tier migration system:
//!
//! ## Database Migrations (global)
//! - Run once per database
//! - Handle extensions, global tables, and database-level setup
//! - Tracked in `db_schema_version` SQL table
//! - Implement `DbMigration` trait
//!
//! ## Graph Migrations (per-graph)
//! - Run once per graph
//! - Handle seed data, indexes on graph tables, and graph-specific schema
//! - Tracked as `:SchemaVersion` node within each graph
//! - Implement `GraphMigration` trait

pub mod db;
pub mod graph;
mod runner;
mod traits;

pub use runner::{run_migrations, MigrationResult};
pub use traits::{DbMigration, GraphMigration, GraphMigrationContext, Migration, Register};
