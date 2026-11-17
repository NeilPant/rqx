use std::path::PathBuf;

/// Where an editor keeps the file this config belongs in, and what a reader has to do
/// with the snippet. An editor whose config is a directory of many files, or whose
/// format cannot be appended to safely, says so rather than pretending.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Home {
    /// Appending to this file is safe: it is a list of settings, and rqx's are new.
    Append(&'static str),
    /// A file rqx must not append to, because the format is a single document and a
    /// second copy of a key is a parse error rather than an override.
    Merge(&'static str),
}

pub struct Client {
    pub slug: &'static str,
    pub name: &'static str,
    pub home: Home,
    pub snippet: &'static str,
}

impl Client {
    pub const ALL: &'static [Self] = &[
        Client {
            slug: "neovim",
            name: "Neovim",
            home: Home::Append("~/.config/nvim/init.lua"),
            snippet: NEOVIM,
        },
        Client {
            slug: "helix",
            name: "Helix",
            home: Home::Merge("~/.config/helix/languages.toml"),
            snippet: HELIX,
        },
        Client {
            slug: "sublime",
            name: "Sublime Text",
            home: Home::Merge("Packages/User/LSP.sublime-settings"),
            snippet: SUBLIME,
        },
        Client {
            slug: "emacs",
            name: "Emacs, with eglot",
            home: Home::Append("~/.emacs.d/init.el"),
            snippet: EMACS,
        },
        Client {
            slug: "vim",
            name: "Vim, with coc.nvim",
            home: Home::Merge("~/.vim/coc-settings.json"),
            snippet: VIM,
        },
        Client {
            slug: "vim-lsp",
            name: "Vim, with vim-lsp",
            home: Home::Append("~/.vimrc"),
            snippet: VIM_LSP,
        },
        Client {
            slug: "lsp-mode",
            name: "Emacs, with lsp-mode",
            home: Home::Append("~/.emacs.d/init.el"),
            snippet: LSP_MODE,
        },
        Client {
            slug: "kakoune",
            name: "Kakoune, with kak-lsp",
            home: Home::Merge("~/.config/kak-lsp/kak-lsp.toml"),
            snippet: KAKOUNE,
        },
        Client {
            slug: "kate",
            name: "Kate",
            home: Home::Merge("~/.config/kate/lspclient/settings.json"),
            snippet: KATE,
        },
    ];

    pub fn parse(slug: &str) -> Option<&'static Self> {
        Self::ALL.iter().find(|client| client.slug == slug)
    }

    /// The file to write into, with `~` expanded.
    pub fn path(&self) -> Option<PathBuf> {
        let raw = match self.home {
            Home::Append(path) | Home::Merge(path) => path,
        };
        let Some(rest) = raw.strip_prefix("~/") else {
            // A path relative to the editor's own package directory, which only the
            // editor knows how to resolve.
            return None;
        };
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join(rest))
    }

    pub fn appendable(&self) -> bool {
        matches!(self.home, Home::Append(_))
    }
}

const NEOVIM: &str = r#"-- rqx
local configs = require("lspconfig.configs")
local lspconfig = require("lspconfig")

if not configs.rqx then
  configs.rqx = {
    default_config = {
      cmd = { "rqx", "lsp" },
      filetypes = { "typescript", "typescriptreact", "python", "go", "java", "cpp" },
      root_dir = lspconfig.util.root_pattern("rqx.yaml", ".git"),
      init_options = { scanOnStartup = true, codeLens = false, patterns = false },
    },
  }
end

lspconfig.rqx.setup({
  on_attach = function(_, buffer)
    local function run(command)
      return function()
        vim.lsp.buf.execute_command({ command = command, arguments = {} })
      end
    end
    local map = function(key, action, description)
      vim.keymap.set("n", "<leader>r" .. key, action, { buffer = buffer, desc = description })
    end
    map("s", run("rqx.scan"), "rqx: scan the workspace")
    map("c", run("rqx.check"), "rqx: review the diff")
    map("p", run("rqx.togglePatterns"), "rqx: toggle pattern advice")
    map("l", run("rqx.toggleCodeLens"), "rqx: toggle code lens")
    map("i", function()
      vim.cmd("botright split | terminal rqx")
    end, "rqx: open the interface")
  end,
})

vim.lsp.handlers["rqx/health"] = function(_, report)
  vim.g.rqx_health = string.format("rqx %d/100", report.health)
end"#;

const HELIX: &str = r#"# languages.toml
[language-server.rqx]
command = "rqx"
args = ["lsp"]

[language-server.rqx.config]
scanOnStartup = true
patterns = false

[[language]]
name = "typescript"
language-servers = ["typescript-language-server", "rqx"]

[[language]]
name = "python"
language-servers = ["pylsp", "rqx"]

[[language]]
name = "go"
language-servers = ["gopls", "rqx"]"#;

const SUBLIME: &str = r#"// LSP.sublime-settings
{
  "clients": {
    "rqx": {
      "enabled": true,
      "command": ["rqx", "lsp"],
      "selector": "source.ts | source.tsx | source.python | source.go | source.java | source.c++",
      "initializationOptions": { "scanOnStartup": true, "patterns": false }
    }
  }
}"#;

const EMACS: &str = r#";; rqx
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               '((typescript-mode typescript-ts-mode python-mode python-ts-mode
                  go-mode go-ts-mode java-mode java-ts-mode c++-mode c++-ts-mode)
                 . ("rqx" "lsp")))
  (defun rqx-scan ()
    (interactive)
    (eglot-execute-command (eglot-current-server) "rqx.scan" []))
  (defun rqx-interface ()
    (interactive)
    (async-shell-command "rqx")))"#;

const VIM: &str = r#"// coc-settings.json
{
  "languageserver": {
    "rqx": {
      "command": "rqx",
      "args": ["lsp"],
      "filetypes": ["typescript", "typescriptreact", "python", "go", "java", "cpp"],
      "rootPatterns": ["rqx.yaml", ".git"],
      "initializationOptions": { "scanOnStartup": true, "patterns": false }
    }
  }
}"#;

const VIM_LSP: &str = r#"" rqx
if executable('rqx')
  au User lsp_setup call lsp#register_server({
    \ 'name': 'rqx',
    \ 'cmd': {server_info->['rqx', 'lsp']},
    \ 'allowlist': ['typescript', 'typescriptreact', 'python', 'go', 'java', 'cpp'],
    \ 'initialization_options': {'scanOnStartup': v:true, 'patterns': v:false},
    \ })
endif"#;

const LSP_MODE: &str = r#";; rqx
(with-eval-after-load 'lsp-mode
  (add-to-list 'lsp-language-id-configuration '(typescript-mode . "typescript"))
  (lsp-register-client
   (make-lsp-client
    :new-connection (lsp-stdio-connection '("rqx" "lsp"))
    :activation-fn (lsp-activate-on "typescript" "typescriptreact" "python"
                                    "go" "java" "cpp")
    ;; `add-on?` is what lets rqx sit beside the language's own server rather
    ;; than replacing it: without it lsp-mode starts one server per buffer.
    :add-on? t
    :server-id 'rqx
    :initialization-options
    (lambda () (list :scanOnStartup t :patterns :json-false))))
  (defun rqx-scan ()
    (interactive)
    (lsp-request "workspace/executeCommand" (list :command "rqx.scan")))
  (defun rqx-interface ()
    (interactive)
    (async-shell-command "rqx")))"#;

const KAKOUNE: &str = r#"# kak-lsp.toml
[language_server.rqx]
command = "rqx"
args = ["lsp"]
filetypes = ["typescript", "tsx", "python", "go", "java", "cpp"]
roots = ["rqx.yaml", ".git"]

[language_server.rqx.settings]
scanOnStartup = true
patterns = false"#;

const KATE: &str = r#"// Settings → LSP Client → User Server Settings
{
  "servers": {
    "rqx": {
      "command": ["rqx", "lsp"],
      "root": "",
      "highlightingModeRegex": "^(TypeScript|Python|Go|Java|C\\+\\+)$"
    }
  }
}"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_client_has_a_snippet_that_mentions_rqx() {
        for client in Client::ALL {
            assert!(!client.snippet.trim().is_empty(), "{}", client.slug);
            assert!(
                client.snippet.contains("rqx"),
                "{} names no server",
                client.slug
            );
            assert_eq!(
                Client::parse(client.slug).map(|c| c.slug),
                Some(client.slug)
            );
        }
    }

    /// Appending to a document format produces a duplicate key rather than a setting,
    /// so only the formats that are lists of statements may be written to.
    #[test]
    fn only_an_appendable_config_offers_to_be_written() {
        for client in Client::ALL {
            match client.home {
                Home::Append(path) => {
                    assert!(client.appendable(), "{}", client.slug);
                    // Lua, Elisp and Vimscript are sequences of statements, so a
                    // second block at the end is a second instruction rather than a
                    // duplicate key.
                    assert!(
                        path.ends_with(".lua") || path.ends_with(".el") || path.ends_with("vimrc"),
                        "{path} is not a format rqx should append to"
                    );
                }
                Home::Merge(_) => {
                    assert!(!client.appendable(), "{}", client.slug);
                }
            }
        }
    }

    #[test]
    fn slugs_are_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for client in Client::ALL {
            assert!(seen.insert(client.slug), "duplicate {}", client.slug);
        }
    }
}
