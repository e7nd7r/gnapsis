//! Minimal Neovim client for msgpack-RPC communication.
//!
//! Provides low-level primitives for communicating with Neovim.
//! Higher-level operations should be implemented in services.

use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

/// Neovim client for RPC communication.
pub struct NvimClient {
    socket_path: PathBuf,
    stream: Option<UnixStream>,
    msgid: AtomicU32,
}

impl NvimClient {
    /// Create a new client with the given socket path.
    pub fn new(socket_path: PathBuf) -> Self {
        Self {
            socket_path,
            stream: None,
            msgid: AtomicU32::new(0),
        }
    }

    /// Try to find and connect to a Neovim socket.
    /// Looks for .nvim/nvim.sock in current directory.
    pub fn try_connect() -> Option<Self> {
        let cwd = std::env::current_dir().ok()?;
        let socket_path = cwd.join(".nvim").join("nvim.sock");

        tracing::debug!(cwd = %cwd.display(), socket = %socket_path.display(), "Looking for nvim socket");

        if socket_path.exists() {
            tracing::debug!("Socket file exists, attempting connection");
            let mut client = Self::new(socket_path);
            match client.connect() {
                Ok(()) => {
                    tracing::info!("Connected to Neovim");
                    return Some(client);
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to connect to nvim socket");
                }
            }
        } else {
            tracing::debug!("Socket file does not exist");
        }
        None
    }

    /// Connect to the Neovim socket.
    pub fn connect(&mut self) -> Result<(), String> {
        match UnixStream::connect(&self.socket_path) {
            Ok(stream) => {
                stream.set_nonblocking(false).ok();
                self.stream = Some(stream);
                Ok(())
            }
            Err(e) => Err(format!("Failed to connect to nvim socket: {}", e)),
        }
    }

    /// Check if connected to Neovim.
    pub fn is_connected(&self) -> bool {
        self.stream.is_some()
    }

    /// Execute Lua code in Neovim and return the result.
    pub fn execute_lua(&mut self, code: &str) -> Result<rmpv::Value, String> {
        self.call(
            "nvim_exec_lua",
            vec![rmpv::Value::String(code.into()), rmpv::Value::Array(vec![])],
        )
    }

    /// Execute a Vim command.
    pub fn command(&mut self, cmd: &str) -> Result<(), String> {
        self.call("nvim_command", vec![rmpv::Value::String(cmd.into())])?;
        Ok(())
    }

    /// Make a raw RPC call to Neovim.
    pub fn call(&mut self, method: &str, args: Vec<rmpv::Value>) -> Result<rmpv::Value, String> {
        let msgid = self.msgid.fetch_add(1, Ordering::SeqCst);
        let stream = self.ensure_connected()?;

        // Build request: [type=0, msgid, method, args]
        let request = rmpv::Value::Array(vec![
            rmpv::Value::Integer(0.into()),
            rmpv::Value::Integer(msgid.into()),
            rmpv::Value::String(method.into()),
            rmpv::Value::Array(args),
        ]);

        // Serialize and send
        let mut buf = Vec::new();
        rmpv::encode::write_value(&mut buf, &request)
            .map_err(|e| format!("Failed to encode request: {}", e))?;

        stream
            .write_all(&buf)
            .map_err(|e| format!("Failed to write to socket: {}", e))?;
        stream
            .flush()
            .map_err(|e| format!("Failed to flush socket: {}", e))?;

        // Read response
        let response = rmpv::decode::read_value(stream)
            .map_err(|e| format!("Failed to read response: {}", e))?;

        // Parse response: [type=1, msgid, error, result]
        if let rmpv::Value::Array(parts) = response {
            if parts.len() >= 4 {
                let err = &parts[2];
                let result = &parts[3];

                if !err.is_nil() {
                    return Err(format!("Neovim error: {:?}", err));
                }
                return Ok(result.clone());
            }
        }

        Err("Invalid response format".to_string())
    }

    /// Ensure connection is established.
    fn ensure_connected(&mut self) -> Result<&mut UnixStream, String> {
        if self.stream.is_none() {
            self.connect()?;
        }
        self.stream
            .as_mut()
            .ok_or_else(|| "No connection".to_string())
    }
}
