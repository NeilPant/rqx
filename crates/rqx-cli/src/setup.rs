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
    /// The editor installs an extension instead of reading a config file.
    Extension(&'static str),
}

pub struct Client {
    pub slug: &'static str,
    pub name: &'static str,
    /// Whether this editor shows the dashboard in a panel of its own.
    pub panel: bool,
    pub home: Home,
    pub snippet: &'static str,
}

impl Client {
    pub const ALL: &'static [Self] = &[
        Client {
            slug: "vscode",
            name: "VS Code, Cursor, Windsurf, VSCodium",
            panel: true,
            home: Home::Extension(
                "Search the marketplace for `rqx`, or `code --install-extension rqx`.",
            ),
            snippet: VSCODE,
        },
        Client {
            slug: "zed",
            name: "Zed",
            panel: false,
            home: Home::Extension("Zed: Extensions, then search for `rqx`."),
            snippet: ZED,
        },
        Client {
            slug: "jetbrains",
            name: "IntelliJ, WebStorm, PyCharm, GoLand, CLion",
            panel: false,
            home: Home::Extension(
                "Install the LSP4IJ plugin, then add rqx as a language server running `rqx lsp`.",
            ),
            snippet: JETBRAINS,
        },
        Client {
            slug: "neovim",
            name: "Neovim",
            panel: false,
            home: Home::Append("~/.config/nvim/init.lua"),
            snippet: NEOVIM,
        },
        Client {
            slug: "helix",
            name: "Helix",
            panel: false,
            home: Home::Merge("~/.config/helix/languages.toml"),
            snippet: HELIX,
        },
        Client {
            slug: "sublime",
            name: "Sublime Text",
            panel: false,
            home: Home::Merge("Packages/User/LSP.sublime-settings"),
            snippet: SUBLIME,
        },
        Client {
            slug: "emacs",
            name: "Emacs, with eglot",
            panel: false,
            home: Home::Append("~/.emacs.d/init.el"),
            snippet: EMACS,
        },
        Client {
            slug: "vim",
            name: "Vim, with coc.nvim",
            panel: false,
            home: Home::Merge("~/.vim/coc-settings.json"),
            snippet: VIM,
        },
        Client {
            slug: "kate",
            name: "Kate",
            panel: false,
            home: Home::Merge("~/.config/kate/lspclient/settings.json"),
            snippet: KATE,
        },
    ];

    pub fn parse(slug: &str) -> Option<&'static Self> {
        Self::ALL.iter().find(|client| client.slug == slug)
    }

    /// The file to write into, with `~` expanded. `None` where the editor installs an
    /// extension instead of reading a config file.
    pub fn path(&self) -> Option<PathBuf> {
        let raw = match self.home {
            Home::Append(path) | Home::Merge(path) => path,
            Home::Extension(_) => return None,
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

const VSCODE: &str = r#"// settings.json — the extension needs none of this to work, but these are the
// switches worth knowing about.
{
  "rqx.scanOnSave": true,
  "rqx.codeLens": false,
  "rqx.patterns": false,
  "rqx.check.base": "origin/HEAD",
  "editor.codeActionsOnSave": { "source.fixAll.rqx": "explicit" }
}"#;

const ZED: &str = r#"// settings.json — the extension launches `rqx lsp` itself; this only tunes it.
{
  "lsp": {
    "rqx": {
      "initialization_options": { "scanOnStartup": true, "patterns": false }
    }
  }
}"#;

const JETBRAINS: &str = r#"# Settings → Languages & Frameworks → Language Servers → New
#   Name:    rqx
#   Command: rqx lsp
#   Filename patterns: *.ts;*.tsx;*.py;*.go;*.java;*.cpp;*.hpp;*.cc;*.h
#
# LSP4IJ works on the Community editions; the built-in LSP API is Ultimate only.
# `rqx serve --open` gives you the dashboard in a browser beside the IDE."#;

const NEOVIM: &str = r#"-- rqx
local configs = require("lspconfig.configs")
local lspconfig = require("lspconfig")

if not configs.rqx then
  configs.rqx = {
    default_config = {
      cmd = { "rqx", "lsp" },
      filetypes = { "typescript", "typescriptreact", "python", "go", "java", "cpp" },
      root_dir = lspconfig.util.root_pattern("RQX.toml", ".git"),
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
    map("d", function()
      vim.fn.jobstart({ "rqx", "serve", "--open" }, { detach = true })
    end, "rqx: open the dashboard")
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
  (defun rqx-dashboard ()
    (interactive)
    (start-process "rqx-serve" nil "rqx" "serve" "--open")))"#;

const VIM: &str = r#"// coc-settings.json
{
  "languageserver": {
    "rqx": {
      "command": "rqx",
      "args": ["lsp"],
      "filetypes": ["typescript", "typescriptreact", "python", "go", "java", "cpp"],
      "rootPatterns": ["RQX.toml", ".git"],
      "initializationOptions": { "scanOnStartup": true, "patterns": false }
    }
  }
}"#;

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
                    assert!(
                        path.ends_with(".lua") || path.ends_with(".el"),
                        "{path} is not a format rqx should append to"
                    );
                }
                Home::Merge(_) | Home::Extension(_) => {
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
