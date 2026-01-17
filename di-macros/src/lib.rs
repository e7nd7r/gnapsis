//! Compile-time dependency injection macros for Gnapsis.
//!
//! This crate provides derive macros for DI:
//! - `#[derive(Context)]` to make a struct's fields extractable
//! - `#[derive(FromContext)]` to auto-resolve fields from a context
//!
//! The `FromRef` trait must be defined in the consuming crate or imported
//! from a shared crate. By default, generated code references `crate::FromRef`.

use proc_macro::TokenStream;

mod context;
mod from_context;

/// Derive macro for creating a DI context.
///
/// When applied to a struct, generates `FromRef` implementations for each
/// field type, allowing them to be extracted from the context.
///
/// # Requirements
///
/// - All fields must implement `Clone`
/// - The struct itself should derive `Clone`
///
/// # Example
///
/// ```ignore
/// use di_macros::{Context, FromRef};
///
/// #[derive(Context, Clone)]
/// pub struct AppContext {
///     pub db: DatabasePool,
///     pub config: AppConfig,
///     pub embedder: Embedder,
/// }
///
/// // Generated implementations:
/// // impl FromRef<AppContext> for DatabasePool { ... }
/// // impl FromRef<AppContext> for AppConfig { ... }
/// // impl FromRef<AppContext> for Embedder { ... }
/// ```
#[proc_macro_derive(Context)]
pub fn derive_context(input: TokenStream) -> TokenStream {
    context::derive_context_impl(input)
}

/// Derive macro for types that can be constructed from a context.
///
/// When applied to a struct, generates a `FromRef<Context>` implementation
/// that resolves each field by calling `FromRef::from_ref` on the context.
///
/// # Requirements
///
/// - Each field type must implement `FromRef<Context>`
/// - The context type defaults to `Context` but can be overridden with
///   `#[from_context(Context = MyContext)]`
///
/// # Example
///
/// ```ignore
/// use di_macros::{FromContext, FromRef};
///
/// #[derive(FromContext, Clone)]
/// pub struct EntityRepository {
///     db: DatabasePool,      // resolved via DatabasePool::from_ref(ctx)
///     config: AppConfig,     // resolved via AppConfig::from_ref(ctx)
/// }
///
/// // Generated implementation:
/// // impl FromRef<Context> for EntityRepository {
/// //     fn from_ref(ctx: &Context) -> Self {
/// //         Self {
/// //             db: DatabasePool::from_ref(ctx),
/// //             config: AppConfig::from_ref(ctx),
/// //         }
/// //     }
/// // }
/// ```
///
/// # Custom Context Type
///
/// ```ignore
/// #[derive(FromContext)]
/// #[from_context(Context = MyAppContext)]
/// pub struct MyRepository {
///     db: Database,
/// }
/// ```
#[proc_macro_derive(FromContext, attributes(from_context))]
pub fn derive_from_context(input: TokenStream) -> TokenStream {
    from_context::derive_from_context_impl(input)
}
