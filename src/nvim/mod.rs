//! Neovim client for IPC communication.
//!
//! Provides a minimal client for communicating with Neovim via Unix socket
//! using msgpack-RPC. Higher-level functionality is provided by services
//! that compose these primitives.
//!
//! # Architecture
//!
//! - `NvimClient`: Low-level primitives (execute_lua, command, call)
//! - `LazyNvimClient`: DI-friendly wrapper with lazy connection
//! - Services (in `crate::services`): High-level operations on top of client

mod client;
mod lazy;

pub use client::NvimClient;
pub use lazy::LazyNvimClient;
