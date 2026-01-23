//! Neovim visualization integration.
//!
//! Provides visualization-specific operations on top of the NvimClient.

pub use crate::nvim::NvimClient;

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

/// Extension trait for visualization operations on NvimClient.
#[allow(dead_code)]
pub trait NvimVisualization {
    /// Open a file in Neovim and highlight a code region.
    fn open_and_highlight(
        &mut self,
        file_path: &str,
        start_line: u32,
        end_line: u32,
        description: &str,
    ) -> Result<(), String>;

    /// Show a persistent bottom panel with document references.
    fn show_references_picker(&mut self, refs: &[DocRefInfo], title: &str) -> Result<(), String>;
}

impl NvimVisualization for NvimClient {
    fn open_and_highlight(
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

    fn show_references_picker(&mut self, refs: &[DocRefInfo], title: &str) -> Result<(), String> {
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
