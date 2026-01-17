//! Dependency injection infrastructure.
//!
//! This module provides compile-time dependency injection using the `FromRef` trait
//! and derive macros from `di-macros`.
//!
//! # Overview
//!
//! - `FromRef<T>`: Trait for extracting a value from a reference to `T`
//! - `#[derive(Context)]`: Makes each field of a struct extractable via `FromRef`
//! - `#[derive(FromContext)]`: Generates `FromRef` impl by resolving each field
//!
//! # Example
//!
//! ```ignore
//! use crate::di::FromRef;
//! use di_macros::{Context, FromContext};
//!
//! #[derive(Context, Clone)]
//! pub struct AppContext {
//!     pub db: DatabasePool,
//!     pub config: Config,
//! }
//!
//! #[derive(FromContext, Clone)]
//! pub struct UserRepository {
//!     db: DatabasePool,  // resolved via FromRef<AppContext>
//! }
//!
//! // Usage
//! let ctx = AppContext { db, config };
//! let repo = UserRepository::from_ref(&ctx);
//! ```

/// Trait for extracting a value from a reference to another type.
///
/// This is the core trait for compile-time dependency injection.
/// Types that implement `FromRef<T>` can be extracted from `&T`.
pub trait FromRef<T> {
    fn from_ref(input: &T) -> Self;
}

/// Blanket implementation: any Clone type can be extracted from itself.
impl<T: Clone> FromRef<T> for T {
    fn from_ref(input: &T) -> Self {
        input.clone()
    }
}

// Re-export derive macros
pub use di_macros::{Context, FromContext};
