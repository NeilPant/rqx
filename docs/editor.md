# rqx in an editor

`rqx lsp` is a Language Server over stdio. It publishes diagnostics on open and on
save, not on every change, so incomplete code is not reported. It answers hover
requests with the measured fact, the reason it matters and the named refactoring that
fixes it.

The server also scans the whole project on its own, once after the handshake and again
after every save. Twelve rules are cross-file rules: unused exports, unused files,
import cycles, duplication, the two co-change rules, and the four performance rules that
compose function costs across calls (`quadratic-via-call`, `deep-recursion`,
`repeated-computation-in-loop` and `pair-scan-could-hash`). They cannot be evaluated from
a single open file, so without the project scan a client would miss them. Diagnostics
for files that were never opened are published in the same way as diagnostics for open
files.

## Commands

Every command is a standard `workspace/executeCommand`, so any client that can invoke
one can bind it. No command depends on an editor-specific extension.

| Command | Arguments | Does |
|---|---|---|
| `rqx.scan` | | Rescan the workspace |
| `rqx.check` | `[base]` | Review the diff against a base (default `origin/HEAD`) and report the gate |
| `rqx.explain` | `[slug]` | The catalogue entry for a rule or a pattern, as markdown |
| `rqx.startHere` | | What to open first, ranked by grade then blast radius |
| `rqx.catalogue` | | Every rule as data |
| `rqx.findings` | `[{severity, rule, category, glob, file}]` | The last scan's findings, filtered as `--severity`, `--rule`, `--category` and `--path` filter them |
| `rqx.plan` | `[budget, limit]` | The work ranked by what it buys against what it costs |
| `rqx.fix` | `[write, files]` | The proven edits; with `write` it applies them and rescans |
| `rqx.verify` | `[base, expect]` | Whether the refactoring named actually happened |
| `rqx.baseline` | `[update]` | Record every current finding, so the gate evaluates only what a change adds |
| `rqx.suppress` | `[{file, line, rule, reason, until}]` | A `WorkspaceEdit` writing an `rqx-ignore` comment. The reason is required: rqx does not honour a waiver without one |
| `rqx.config` | | The effective settings and where each one came from |
| `rqx.disableRule` / `rqx.disableCategory` | `[name]` | A `WorkspaceEdit` that switches one off in `rqx.yaml` |
| `rqx.togglePatterns` | | Half-applied design patterns on or off, as hints |
| `rqx.toggleCodeLens` | | The per-file grade lens on or off |

`rqx.scan`, `rqx.check`, `rqx.fix`, `rqx.verify` and `rqx.baseline` run a scan, so they
respond when the scan finishes. A second request of the same kind replaces the first
and answers it with `{ "coalesced": true }`.

The suggested shortcuts are the same in every editor: a leader chord followed by one
key. Only the leader key is claimed in each editor. The second keys are `s` scan,
`c` check, `e` explain, `h` start here, `d` disable, `p` patterns, `l` lens, `o` panel,
`i` the interface in a terminal split, `f` fix the file, `x` waive and `v` verify.

## Notifications

After every scan the server sends `rqx/health` (`{health, findings, files, elapsedMs}`)
for a status line, and `rqx/report`. The report carries the same projection that
`--reporter json` writes (grades, counts, impact, the start-here ranking and pattern
advice), so an editor and CI see the same scan results. A client that does not
recognise these notifications ignores them.

A review sends both notifications as well, with the verdict under `review`. The review
scans the whole project to answer its narrower question, so the numbers a panel shows
would otherwise be out of date until the next save.

Every diagnostic carries the remaining fields on `data`:

```json
{ "rule": "long-function", "category": "size", "severity": "warning",
  "evidence": "99 statements (threshold 40)", "confidence": "measured threshold",
  "autofixable": false, "known": false, "technique": ["Extract Method"] }
```

`severity` is repeated on `data` because a client may cap the severity it renders.
`autofixable` reflects the project's `fix` configuration, not only the rule. The two
differ when the configuration narrows which fixes may be applied.

## Settings

Pass these as `initializationOptions`. No client-side plugin is required.

| Option | Default | Meaning |
|---|---|---|
| `configFile` | *(unset)* | A YAML config to use instead of the one rqx finds itself |
| `scanOnStartup` | `true` | Scan the workspace after the handshake |
| `scanOnSave` | `true` | Rescan the workspace after every save. The result of a cross-file rule can change when a different file is saved |
| `codeLens` | `false` | A grade, blast radius and churn lens above the first line |
| `patterns` | `false` | Publish half-applied design patterns as hints |

`rqx.yaml` is otherwise resolved in the same way as by the CLI. The server reloads it
without a restart when the file changes, if the client sends
`workspace/didChangeWatchedFiles` for `**/rqx.yaml`.

## Neovim

With `nvim-lspconfig`:

```lua
local configs = require("lspconfig.configs")
local lspconfig = require("lspconfig")

if not configs.rqx then
  configs.rqx = {
    default_config = {
      cmd = { "rqx", "lsp" },
      filetypes = {
        "typescript", "typescriptreact", "javascript", "javascriptreact",
        "python", "go", "java", "cpp",
      },
      root_dir = lspconfig.util.root_pattern("rqx.yaml", ".git"),
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

`root_pattern` lists `rqx.yaml` first, so a monorepo package with its own `rqx.yaml`
takes precedence over the repository root. The health score is available on
`rqx/health`:

```lua
vim.lsp.handlers["rqx/health"] = function(_, report)
  vim.g.rqx_health = string.format("rqx %d/100", report.health)
end
```

With Mason, `:MasonInstall rqx` installs the binary.

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
name = "javascript"
language-servers = ["typescript-language-server", "rqx"]

[[language]]
name = "python"
language-servers = ["pylsp", "rqx"]
```

Add `rqx` to the `language-servers` list of any of the six languages. rqx runs
alongside the language's own server and does not replace it: rqx reports nothing about
types or syntax errors.

The commands are bound in `config.toml`. `space` is Helix's leader key, so the
bindings use `space r`:

```toml
[keys.normal.space.r]
s = ":lsp-workspace-command rqx.scan"
c = ":lsp-workspace-command rqx.check"
h = ":lsp-workspace-command rqx.startHere"
p = ":lsp-workspace-command rqx.togglePatterns"
l = ":lsp-workspace-command rqx.toggleCodeLens"
```

## Sublime Text

With [`sublimelsp/LSP`](https://github.com/sublimelsp/LSP), in `LSP.sublime-settings`:

```json
{
  "clients": {
    "rqx": {
      "enabled": true,
      "command": ["rqx", "lsp"],
      "selector": "source.ts | source.tsx | source.js | source.jsx | source.python | source.go | source.java | source.c++",
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
             '((typescript-ts-mode js-ts-mode python-ts-mode go-ts-mode java-ts-mode c++-ts-mode)
               . ("rqx" "lsp")))

(defun rqx-command (name)
  (eglot-execute-command (eglot-current-server) name []))

(keymap-global-set "C-c r s" (lambda () (interactive) (rqx-command "rqx.scan")))
(keymap-global-set "C-c r c" (lambda () (interactive) (rqx-command "rqx.check")))
(keymap-global-set "C-c r h" (lambda () (interactive) (rqx-command "rqx.startHere")))
```

`C-c` followed by a letter is reserved for user bindings in Emacs, so these bindings do
not collide with built-in ones.

## Vim, with coc.nvim

In `coc-settings.json`:

```json
{
  "languageserver": {
    "rqx": {
      "command": "rqx",
      "args": ["lsp"],
      "filetypes": ["typescript", "typescriptreact", "javascript", "javascriptreact", "python", "go", "java", "cpp"],
      "rootPatterns": ["rqx.yaml", ".git"],
      "initializationOptions": { "scanOnStartup": true }
    }
  }
}
```

coc exposes every command in `executeCommandProvider` as a `:CocCommand`, so
`:CocCommand rqx.scan` works without further setup:

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
    \ 'allowlist': ['typescript', 'javascript', 'python', 'go', 'java', 'cpp'],
    \ 'initialization_options': {'scanOnStartup': v:true},
    \ })
endif
```

## Emacs, with lsp-mode

```elisp
(with-eval-after-load 'lsp-mode
  (lsp-register-client
   (make-lsp-client
    :new-connection (lsp-stdio-connection '("rqx" "lsp"))
    :activation-fn (lsp-activate-on "typescript" "typescriptreact" "javascript"
                                    "javascriptreact" "python" "go" "java" "cpp")
    :add-on? t
    :server-id 'rqx
    :initialization-options
    (lambda () (list :scanOnStartup t :patterns :json-false)))))
```

`:add-on? t` lets rqx run beside the language's own server. Without it, lsp-mode
treats the two servers as alternatives and starts whichever it finds first.

## Kakoune, with kak-lsp

```toml
# kak-lsp.toml
[language_server.rqx]
command = "rqx"
args = ["lsp"]
filetypes = ["typescript", "tsx", "javascript", "jsx", "python", "go", "java", "cpp"]
roots = ["rqx.yaml", ".git"]

[language_server.rqx.settings]
scanOnStartup = true
patterns = false
```

## Kate

In Kate's **Settings → LSP Client → User Server Settings**:

```json
{
  "servers": {
    "rqx": {
      "command": ["rqx", "lsp"],
      "highlightingModeRegex": "^(TypeScript|JavaScript|Python|Go|Java|C\\+\\+)$",
      "initializationOptions": { "scanOnStartup": true }
    }
  }
}
```

## Anything else

Any client that speaks LSP over stdio can use the server. Run `rqx lsp` with the
project root as the working directory, or pass the root as a positional argument. The
`--stdio` flag is accepted and ignored, because several clients pass it by convention.

The server advertises `textDocumentSync` (open, change and save), `hoverProvider`,
`codeActionProvider`, `codeLensProvider` and `executeCommandProvider`. For a finding
whose fix requires a decision, the server offers to disable the rule or explain it. For
a finding whose fix is determined by the finding alone, it offers the edit, tagged
`source.fixAll` so that an editor can apply all such edits on save. A source edit is
offered only after the same verification that `rqx fix` runs, so an edit accepted in
an editor has been verified to the same degree as one applied on the command line.
