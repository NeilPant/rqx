# rqx in an editor

`rqx lsp` is a Language Server over stdio. It publishes diagnostics on open and on
save — not on every keystroke, because a half-typed function is a work in progress
rather than a finding — and answers hover with the measured fact, the reason it
matters and the named refactoring that fixes it.

It also answers one request outside the protocol, `rqx/scanWorkspace`, which runs a
full scan and returns `{health, findings, files, elapsedMs}`. The cross-file rules —
unused exports, unused files, import cycles — cannot be answered from one open file,
so an editor that wants them has to ask.

Configuration comes from `RQX.toml`, resolved the same way the CLI resolves it. The
server reads it once at startup, so an editor should restart the server when the file
changes.

## VS Code

[`editors/vscode`](../editors/vscode/README.md) — install it, or build a `.vsix` with
`npm ci && npm run package`.

## Neovim

With `nvim-lspconfig`:

```lua
local configs = require("lspconfig.configs")
local lspconfig = require("lspconfig")

if not configs.rqx then
  configs.rqx = {
    default_config = {
      cmd = { "rqx", "lsp" },
      filetypes = { "typescript", "typescriptreact", "python", "go", "java", "cpp", "c" },
      root_dir = lspconfig.util.root_pattern("RQX.toml", ".git"),
      settings = {},
    },
  }
end

lspconfig.rqx.setup({})
```

`root_pattern` puts `RQX.toml` first so a monorepo package with its own config wins
over the repository root.

## Helix

In `languages.toml`:

```toml
[language-server.rqx]
command = "rqx"
args = ["lsp"]

[[language]]
name = "typescript"
language-servers = ["typescript-language-server", "rqx"]

[[language]]
name = "python"
language-servers = ["pylsp", "rqx"]
```

Add `rqx` to the `language-servers` list of any of the five languages. It is a
second opinion alongside the language's own server rather than a replacement: rqx
reports nothing about types or syntax errors.

## Zed

In `settings.json`:

```json
{
  "lsp": {
    "rqx": { "binary": { "path": "rqx", "arguments": ["lsp"] } }
  }
}
```

## Anything else

Any client that speaks LSP over stdio works: run `rqx lsp` with the project root as
the working directory, or pass the root as a positional argument. The server advertises
`textDocumentSync` (open and save only) and `hoverProvider`, and nothing else — there
are no code actions, because every fix rqx names is a refactoring rather than an edit
it could apply for you.
