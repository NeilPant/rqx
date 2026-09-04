# rqx in an editor

`rqx lsp` is a Language Server over stdio. It publishes diagnostics on open and on save
— not on every keystroke, because a half-typed function is a work in progress rather
than a finding — and answers hover with the measured fact, the reason it matters and the
named refactoring that fixes it.

It also scans the whole project on its own: once after the handshake, and again after
every save. The eight cross-file rules — unused exports, unused files, import cycles,
duplication, and the two co-change rules — cannot be answered from one open file, so
without that a client would silently see 52 of the 62 rules. Diagnostics for files you
never opened are published the same way as the ones for files you did.

## Commands

Every command is a standard `workspace/executeCommand`, so any client that can invoke one
can bind it. Nothing here is a VS Code extension talking to itself.

| Command | Does |
|---|---|
| `rqx.scan` | Rescan the workspace |
| `rqx.check` | Review the diff against a base (default `origin/HEAD`) and report the gate |
| `rqx.explain` | The catalogue entry for a rule slug, as markdown |
| `rqx.startHere` | What to open first, ranked by grade then blast radius |
| `rqx.catalogue` | Every rule as data |
| `rqx.disableRule` / `rqx.disableCategory` | A `WorkspaceEdit` that switches one off in `RQX.toml` |
| `rqx.togglePatterns` | Half-applied design patterns on or off, as hints |
| `rqx.toggleCodeLens` | The per-file grade lens on or off |

The same shortcuts are suggested everywhere: a leader chord, then `s` scan, `c` check,
`e` explain, `h` start here, `d` disable, `p` patterns, `l` lens, `o` panel. One key is
claimed rather than eight, and the second key is the same in every editor.

## Notifications

After every scan the server sends `rqx/health` (`{health, findings, files, elapsedMs}`)
for a status line, and `rqx/report`, which carries the same projection `--reporter json`
writes — grades, counts, impact, the start-here ranking and pattern advice — so an editor
and CI cannot disagree about what the scan found. A client that does not know them
ignores them.

## Settings

Pass these as `initializationOptions`; no client-side plugin is needed for any of them.

| Option | Default | Meaning |
|---|---|---|
| `configFile` | *(unset)* | An `RQX.toml` to use instead of the one rqx finds itself |
| `scanOnStartup` | `true` | Scan the workspace after the handshake |
| `codeLens` | `false` | A grade, blast radius and churn lens above the first line |
| `patterns` | `false` | Publish half-applied design patterns as hints |

`RQX.toml` is otherwise resolved exactly as the CLI resolves it, and the server reloads
it in place when the file changes — send `workspace/didChangeWatchedFiles` for
`**/RQX.toml` and no restart is needed.

## VS Code

[`editors/vscode`](../editors/vscode/README.md) — install it from the Marketplace or Open
VSX and the matching server comes with it; nothing else to set up. The leader chord is
`cmd+alt+r` on macOS and `ctrl+alt+r` elsewhere.

## Neovim

With `nvim-lspconfig`:

```lua
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
    local function run(command, arguments)
      return function()
        vim.lsp.buf.execute_command({ command = command, arguments = arguments or {} })
      end
    end
    local map = function(key, action, description)
      vim.keymap.set("n", "<leader>r" .. key, action, { buffer = buffer, desc = description })
    end
    map("s", run("rqx.scan"), "rqx: scan the workspace")
    map("c", run("rqx.check"), "rqx: review the diff")
    map("p", run("rqx.togglePatterns"), "rqx: toggle pattern advice")
    map("l", run("rqx.toggleCodeLens"), "rqx: toggle code lens")
    map("h", function()
      vim.lsp.buf_request(buffer, "workspace/executeCommand",
        { command = "rqx.startHere", arguments = {} },
        function(_, ranked)
          if ranked and ranked[1] then vim.cmd.edit(ranked[1].file) end
        end)
    end, "rqx: open what matters first")
  end,
})
```

`root_pattern` puts `RQX.toml` first so a monorepo package with its own config wins over
the repository root. The health score is on `rqx/health`:

```lua
vim.lsp.handlers["rqx/health"] = function(_, report)
  vim.g.rqx_health = string.format("rqx %d/100", report.health)
end
```

`:MasonInstall rqx` fetches the binary if you use Mason.

## Helix

In `languages.toml`:

```toml
[language-server.rqx]
command = "rqx"
args = ["lsp"]
config = { scanOnStartup = true }

[[language]]
name = "typescript"
language-servers = ["typescript-language-server", "rqx"]

[[language]]
name = "python"
language-servers = ["pylsp", "rqx"]
```

Add `rqx` to the `language-servers` list of any of the five languages. It is a second
opinion alongside the language's own server rather than a replacement: rqx reports
nothing about types or syntax errors.

The commands bind in `config.toml` — `space` is already Helix's leader, so `space r`:

```toml
[keys.normal.space.r]
s = ":lsp-workspace-command rqx.scan"
c = ":lsp-workspace-command rqx.check"
h = ":lsp-workspace-command rqx.startHere"
p = ":lsp-workspace-command rqx.togglePatterns"
l = ":lsp-workspace-command rqx.toggleCodeLens"
```

## Zed

In `settings.json`:

```json
{
  "lsp": {
    "rqx": {
      "binary": { "path": "rqx", "arguments": ["lsp"] },
      "initialization_options": { "scanOnStartup": true }
    }
  }
}
```

Zed gets diagnostics, hover, code actions and — with `"code_lens": true` — the grade
lens. It does not get the commands bound to keys: a Zed extension can contribute a
language server, a theme, an MCP server or a debugger, and nothing else, so there is no
place to register them.

## Sublime Text

With [`sublimelsp/LSP`](https://github.com/sublimelsp/LSP), in `LSP.sublime-settings`:

```json
{
  "clients": {
    "rqx": {
      "enabled": true,
      "command": ["rqx", "lsp"],
      "selector": "source.ts | source.tsx | source.python | source.go | source.java | source.c++",
      "initializationOptions": { "scanOnStartup": true }
    }
  }
}
```

And in `Default.sublime-keymap`:

```json
[
  { "keys": ["ctrl+alt+r", "s"], "command": "lsp_execute",
    "args": { "session_name": "rqx", "command_name": "rqx.scan" } },
  { "keys": ["ctrl+alt+r", "c"], "command": "lsp_execute",
    "args": { "session_name": "rqx", "command_name": "rqx.check" } },
  { "keys": ["ctrl+alt+r", "h"], "command": "lsp_execute",
    "args": { "session_name": "rqx", "command_name": "rqx.startHere" } }
]
```

## Emacs

With `eglot`:

```elisp
(add-to-list 'eglot-server-programs
             '((typescript-ts-mode python-ts-mode go-ts-mode java-ts-mode c++-ts-mode)
               . ("rqx" "lsp")))

(defun rqx-command (name)
  (eglot-execute-command (eglot-current-server) name []))

(keymap-global-set "C-c r s" (lambda () (interactive) (rqx-command "rqx.scan")))
(keymap-global-set "C-c r c" (lambda () (interactive) (rqx-command "rqx.check")))
(keymap-global-set "C-c r h" (lambda () (interactive) (rqx-command "rqx.startHere")))
```

`C-c` followed by a letter is the prefix Emacs reserves for users, so nothing of yours
is taken.

## Vim, with coc.nvim

In `coc-settings.json`:

```json
{
  "languageserver": {
    "rqx": {
      "command": "rqx",
      "args": ["lsp"],
      "filetypes": ["typescript", "typescriptreact", "python", "go", "java", "cpp"],
      "rootPatterns": ["RQX.toml", ".git"],
      "initializationOptions": { "scanOnStartup": true }
    }
  }
}
```

coc surfaces everything in `executeCommandProvider` as a `:CocCommand`, so
`:CocCommand rqx.scan` works with no further setup:

```vim
nnoremap <leader>rs :CocCommand rqx.scan<CR>
nnoremap <leader>rc :CocCommand rqx.check<CR>
nnoremap <leader>rh :CocCommand rqx.startHere<CR>
```

## Vim, with vim-lsp

```vim
if executable('rqx')
  au User lsp_setup call lsp#register_server({
    \ 'name': 'rqx',
    \ 'cmd': {server_info->['rqx', 'lsp']},
    \ 'allowlist': ['typescript', 'python', 'go', 'java', 'cpp'],
    \ 'initialization_options': {'scanOnStartup': v:true},
    \ })
endif
```

## Kate

In Kate's **Settings → LSP Client → User Server Settings**:

```json
{
  "servers": {
    "rqx": {
      "command": ["rqx", "lsp"],
      "highlightingModeRegex": "^(TypeScript|Python|Go|Java|C\\+\\+)$",
      "initializationOptions": { "scanOnStartup": true }
    }
  }
}
```

## Anything else

Any client that speaks LSP over stdio works: run `rqx lsp` with the project root as the
working directory, or pass the root as a positional argument. A `--stdio` flag is
accepted and ignored, because several clients pass it by convention.

The server advertises `textDocumentSync` (open, change and save), `hoverProvider`,
`codeActionProvider`, `codeLensProvider` and `executeCommandProvider`. A finding whose
fix is a decision offers to disable the rule or explain it; a finding whose fix is
decided by the finding alone offers the edit, tagged `source.fixAll` so an editor can
apply every one of them on save. A source edit is only offered after the same
verification `rqx fix` runs, so accepting one in an editor is no less proven than
accepting one on the command line.
