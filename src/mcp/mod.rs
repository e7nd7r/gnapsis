//! Model Context Protocol (MCP) server implementation for Gnapsis.
//!
//! This module provides an MCP server that enables AI assistants to interact
//! with the code intelligence graph stored in Neo4j.
//!
//! ## Architecture
//!
//! The server uses compile-time dependency injection via the `Context` struct.
//! Repositories are resolved at tool execution time using `FromRef`.
//!
//! ## Modules
//!
//! - `server`: MCP server implementation with tool router
//! - `tools`: Tool implementations organized by domain

pub(crate) mod server;
mod tools;

pub use server::McpServer;
