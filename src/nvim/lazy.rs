//! Lazy-loaded Neovim client for dependency injection.
//!
//! Provides a thread-safe wrapper that only connects to Neovim when
//! actually needed. This allows the client to be injected via DI
//! without requiring Neovim to be running at startup.

use std::sync::{Arc, Mutex, MutexGuard};

use super::NvimClient;

/// Lazy-loaded Neovim client wrapper.
///
/// This wrapper defers connection to Neovim until the first use.
/// It's designed for dependency injection where Neovim may or may
/// not be available. Each call attempts to connect if not currently
/// connected, allowing recovery after Neovim starts.
///
/// # Example
///
/// ```ignore
/// let lazy = LazyNvimClient::new();
///
/// // Connection happens on first use
/// lazy.with_client(|client| {
///     client.execute_lua("print('hello')")
/// })?;
/// ```
#[derive(Clone)]
pub struct LazyNvimClient {
    inner: Arc<Mutex<Option<NvimClient>>>,
}

impl Default for LazyNvimClient {
    fn default() -> Self {
        Self::new()
    }
}

impl LazyNvimClient {
    /// Create a new lazy client.
    ///
    /// No connection is attempted until `with_client` is called.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
        }
    }

    /// Execute a function with the NvimClient.
    ///
    /// Attempts to connect if not currently connected. Each call will
    /// retry connection if Neovim wasn't available previously, allowing
    /// the system to recover when Neovim becomes available.
    pub fn with_client<F, R>(&self, f: F) -> Result<R, String>
    where
        F: FnOnce(&mut NvimClient) -> Result<R, String>,
    {
        let mut guard = self.lock()?;

        // Try to connect if not connected
        if guard.is_none() {
            tracing::debug!("LazyNvimClient: not connected, attempting connection");
            if let Some(client) = NvimClient::try_connect() {
                tracing::debug!("LazyNvimClient: connection successful");
                *guard = Some(client);
            } else {
                tracing::debug!("LazyNvimClient: connection failed");
                return Err("Neovim not available (no socket found)".to_string());
            }
        }

        // Execute with connected client
        let client = guard.as_mut().expect("just connected");
        f(client)
    }

    /// Check if Neovim is currently connected.
    ///
    /// Returns `Some(true)` if connected, `Some(false)` if not connected.
    /// Does not attempt to connect.
    pub fn is_available(&self) -> Option<bool> {
        let guard = self.inner.lock().ok()?;
        Some(guard.is_some())
    }

    /// Disconnect from Neovim.
    ///
    /// Next `with_client` call will attempt to reconnect.
    pub fn disconnect(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            *guard = None;
        }
    }

    fn lock(&self) -> Result<MutexGuard<'_, Option<NvimClient>>, String> {
        self.inner.lock().map_err(|_| "Lock poisoned".to_string())
    }
}
