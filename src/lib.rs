//! Gnapsis - Code Intelligence Graph MCP Server
//!
//! A knowledge graph for understanding codebases through semantic relationships.

pub mod cli;
pub mod config;
pub mod context;
pub mod di;
pub mod error;
pub mod git;
pub mod graph;
pub mod mcp;
pub mod migrations;
pub mod models;
pub mod nvim;
pub mod repositories;
pub mod services;
pub mod visualization;

// Re-export FromRef at crate root for di-macros generated code
pub use di::FromRef;
