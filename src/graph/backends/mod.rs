//! Backend implementations for different graph databases.
//!
//! Each backend implements the core traits from [`crate::graph::traits`]:
//!
//! - [`CypherExecutor`](crate::graph::CypherExecutor) - Required
//! - [`SqlExecutor`](crate::graph::SqlExecutor) - Optional
//! - [`Transaction`](crate::graph::Transaction) - Required
//! - [`GraphClient`](crate::graph::GraphClient) - Required
//!
//! # Available Backends
//!
//! | Backend | Module | Status |
//! |---------|--------|--------|
//! | PostgreSQL + Apache AGE | [`postgres`] | Available |
//! | SQLite + graphqlite | `sqlite` | Future |
//!
//! # Implementing a Backend
//!
//! To implement a new backend:
//!
//! 1. Create a client struct (e.g., `PostgresClient`)
//! 2. Create a transaction struct (e.g., `PostgresTransaction`)
//! 3. Implement `CypherExecutor` for both
//! 4. Implement `Transaction` for the transaction struct
//! 5. Implement `GraphClient` for the client struct
//! 6. Optionally implement `SqlExecutor` if the backend supports SQL

pub mod postgres;

// Future: mod sqlite;
