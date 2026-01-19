//! Neovim client for visualization integration.
//!
//! Communicates with Neovim via Unix socket using msgpack-RPC.

use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

/// Information about a document reference for the picker.
#[derive(Debug, Clone)]
pub struct DocRefInfo {
    /// File path (relative to project root).
    pub path: String,
    /// Starting line number.
    pub start_line: u32,
    /// Ending line number.
    pub end_line: u32,
    /// Description of what this reference points to.
    pub description: String,
}

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

        if socket_path.exists() {
            let mut client = Self::new(socket_path);
            if client.connect().is_ok() {
                return Some(client);
            }
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

    /// Ensure connection is established.
    fn ensure_connected(&mut self) -> Result<&mut UnixStream, String> {
        if self.stream.is_none() {
            self.connect()?;
        }
        self.stream
            .as_mut()
            .ok_or_else(|| "No connection".to_string())
    }

    /// Execute Lua code in Neovim.
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

    /// Make an RPC call to Neovim.
    fn call(&mut self, method: &str, args: Vec<rmpv::Value>) -> Result<rmpv::Value, String> {
        // Get msgid before borrowing stream
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

    /// Open a file in Neovim and highlight a code region.
    pub fn open_and_highlight(
        &mut self,
        file_path: &str,
        start_line: u32,
        end_line: u32,
        description: &str,
    ) -> Result<(), String> {
        // Escape strings for Lua
        let escaped_path = file_path.replace('\\', "\\\\").replace('"', "\\\"");
        let escaped_desc = description
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n");

        let lua_code = format!(
            r#"
            local filepath = "{}"
            local start_line = {}
            local end_line = {}
            local description = "{}"

            -- Make path absolute if relative
            if not vim.startswith(filepath, '/') then
                filepath = vim.fn.getcwd() .. '/' .. filepath
            end

            -- Open file
            vim.cmd('edit ' .. vim.fn.fnameescape(filepath))

            -- Get buffer
            local bufnr = vim.api.nvim_get_current_buf()

            -- Clear previous highlights
            local ns = vim.api.nvim_create_namespace('gnapsis-viz')
            vim.api.nvim_buf_clear_namespace(bufnr, ns, 0, -1)

            -- Highlight the region
            for line = start_line, end_line do
                vim.api.nvim_buf_add_highlight(bufnr, ns, 'Visual', line - 1, 0, -1)
            end

            -- Jump to start line and center
            vim.api.nvim_win_set_cursor(0, {{ start_line, 0 }})
            vim.cmd('normal! zz')

            -- Show description in echo area
            vim.api.nvim_echo({{ {{ '📍 ' .. description, 'Comment' }} }}, false, {{}})

            return true
            "#,
            escaped_path, start_line, end_line, escaped_desc
        );

        self.execute_lua(&lua_code)?;
        Ok(())
    }

    /// Show a persistent bottom panel with document references.
    ///
    /// Creates a horizontal split at the bottom that stays open while
    /// the user navigates between files. Press number keys or Enter to
    /// jump to a reference, 'q' to close the panel.
    pub fn show_references_picker(
        &mut self,
        refs: &[DocRefInfo],
        title: &str,
    ) -> Result<(), String> {
        if refs.is_empty() {
            return Ok(());
        }

        // Build Lua table of references
        let refs_lua: Vec<String> = refs
            .iter()
            .map(|r| {
                let escaped_path = r.path.replace('\\', "\\\\").replace('"', "\\\"");
                let escaped_desc = r
                    .description
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace('\n', " ");
                format!(
                    r#"{{ path = "{}", start_line = {}, end_line = {}, desc = "{}" }}"#,
                    escaped_path, r.start_line, r.end_line, escaped_desc
                )
            })
            .collect();

        let refs_table = refs_lua.join(", ");
        let escaped_title = title.replace('\\', "\\\\").replace('"', "\\\"");

        let lua_code = format!(
            r##"
            local refs = {{ {refs_table} }}
            local title = "{escaped_title}"
            local hl_ns = vim.api.nvim_create_namespace('gnapsis-viz')
            local panel_ns = vim.api.nvim_create_namespace('gnapsis-panel')

            -- Store state in a global table
            _G.gnapsis_refs = _G.gnapsis_refs or {{}}
            local state = _G.gnapsis_refs

            -- Function to open and highlight a reference
            local function open_ref(ref)
                -- Switch to main editor window first
                if state.editor_win and vim.api.nvim_win_is_valid(state.editor_win) then
                    vim.api.nvim_set_current_win(state.editor_win)
                end

                local filepath = ref.path
                if not vim.startswith(filepath, '/') then
                    filepath = vim.fn.getcwd() .. '/' .. filepath
                end

                vim.cmd('edit ' .. vim.fn.fnameescape(filepath))

                local bufnr = vim.api.nvim_get_current_buf()
                vim.api.nvim_buf_clear_namespace(bufnr, hl_ns, 0, -1)

                for line = ref.start_line, ref.end_line do
                    pcall(vim.api.nvim_buf_add_highlight, bufnr, hl_ns, 'Visual', line - 1, 0, -1)
                end

                vim.api.nvim_win_set_cursor(0, {{ ref.start_line, 0 }})
                vim.cmd('normal! zz')
            end

            -- Function to close the panel
            local function close_panel()
                if state.winnr and vim.api.nvim_win_is_valid(state.winnr) then
                    vim.api.nvim_win_close(state.winnr, true)
                end
                if state.bufnr and vim.api.nvim_buf_is_valid(state.bufnr) then
                    vim.api.nvim_buf_delete(state.bufnr, {{ force = true }})
                end
                state.winnr = nil
                state.bufnr = nil
                state.refs = nil
            end

            -- Close existing panel if any
            close_panel()

            -- Save current editor window
            state.editor_win = vim.api.nvim_get_current_win()

            -- Create buffer for the panel
            state.bufnr = vim.api.nvim_create_buf(false, true)
            vim.api.nvim_buf_set_name(state.bufnr, 'gnapsis://references')
            vim.bo[state.bufnr].buftype = 'nofile'
            vim.bo[state.bufnr].bufhidden = 'wipe'
            vim.bo[state.bufnr].swapfile = false
            vim.bo[state.bufnr].filetype = 'gnapsis-refs'

            -- Store refs for navigation
            state.refs = refs

            -- Build panel content
            local lines = {{}}
            table.insert(lines, '# ' .. title)
            table.insert(lines, '')
            for i, ref in ipairs(refs) do
                local line = string.format('  [%d] %s', i, ref.desc)
                table.insert(lines, line)
                table.insert(lines, string.format('      %s:%d-%d', ref.path, ref.start_line, ref.end_line))
            end
            table.insert(lines, '')
            table.insert(lines, '─────────────────────────────────────────────────')
            table.insert(lines, '  [1-9] jump to ref   [Enter] jump to ref under cursor   [q] close')

            -- Set content
            vim.api.nvim_buf_set_lines(state.bufnr, 0, -1, false, lines)
            vim.bo[state.bufnr].modifiable = false

            -- Create bottom split
            local height = math.min(#lines + 1, 15)
            vim.cmd('botright ' .. height .. 'split')
            state.winnr = vim.api.nvim_get_current_win()
            vim.api.nvim_win_set_buf(state.winnr, state.bufnr)

            -- Window options
            vim.wo[state.winnr].number = false
            vim.wo[state.winnr].relativenumber = false
            vim.wo[state.winnr].signcolumn = 'no'
            vim.wo[state.winnr].winfixheight = true
            vim.wo[state.winnr].cursorline = true

            -- Add highlights
            vim.api.nvim_buf_add_highlight(state.bufnr, panel_ns, 'Title', 0, 0, -1)
            for i = 1, #refs do
                local line_idx = 1 + (i - 1) * 2 + 1
                vim.api.nvim_buf_add_highlight(state.bufnr, panel_ns, 'Function', line_idx, 0, 5)
                vim.api.nvim_buf_add_highlight(state.bufnr, panel_ns, 'String', line_idx, 5, -1)
                vim.api.nvim_buf_add_highlight(state.bufnr, panel_ns, 'Comment', line_idx + 1, 0, -1)
            end

            -- Keymaps
            local opts = {{ buffer = state.bufnr, silent = true }}

            vim.keymap.set('n', 'q', close_panel, opts)
            vim.keymap.set('n', '<Esc>', close_panel, opts)

            -- Number keys to jump directly
            for i = 1, 9 do
                vim.keymap.set('n', tostring(i), function()
                    if state.refs[i] then
                        open_ref(state.refs[i])
                    end
                end, opts)
            end

            -- Enter to jump to ref under cursor
            vim.keymap.set('n', '<CR>', function()
                local cursor = vim.api.nvim_win_get_cursor(state.winnr)
                local line = cursor[1]
                -- Lines are: title, blank, then pairs of (desc, path) for each ref
                local ref_idx = math.floor((line - 2) / 2) + 1
                if ref_idx >= 1 and ref_idx <= #state.refs then
                    open_ref(state.refs[ref_idx])
                end
            end, opts)

            -- Return focus to editor
            vim.api.nvim_set_current_win(state.editor_win)

            -- If only one reference, open it directly
            if #refs == 1 then
                open_ref(refs[1])
            end

            return true
            "##,
            refs_table = refs_table,
            escaped_title = escaped_title
        );

        self.execute_lua(&lua_code)?;
        Ok(())
    }
}
