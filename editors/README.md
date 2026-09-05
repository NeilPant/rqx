# rqx in editors

The server is the product; these are configuration.

Everything an editor shows comes from `rqx lsp` over standard LSP — diagnostics, hovers,
quick fixes, the code lens, and a command set any client can bind. A capability added to
the server appears in every editor in the same commit, which is why the clients here stay
small enough to survive being unmaintained.

## What each editor gets

| | Diagnostics | Cross-file rules | Hover | Quick fixes | Code lens | Commands | Panel |
|---|---|---|---|---|---|---|---|
| **VS Code** (+ Cursor, Windsurf, VSCodium) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **JetBrains** (IDEA, WebStorm, PyCharm, GoLand, CLion) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | tool window |
| **Neovim** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | — |
| **Helix** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | — |
| **Sublime Text** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | — |
| **Emacs** (eglot, lsp-mode) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | — |
| **Vim** (coc.nvim, vim-lsp) | ✓ | ✓ | ✓ | ✓ | — | coc only | — |
| **Zed** | ✓ | ✓ | ✓ | ✓ | ✓ | — | — |
| **Kate** | ✓ | ✓ | ✓ | ✓ | — | — | — |

**Cross-file rules** are the eight that cannot be answered from one open file — unused
exports, unused files, import cycles, duplication and the two co-change rules. The server
scans the whole project after startup and after every save and publishes for files the
client never opened, so no editor has to know a custom request to get them.

**Commands** are all plain `workspace/executeCommand`: `rqx.scan`, `rqx.check`,
`rqx.explain`, `rqx.startHere`, `rqx.catalogue`, `rqx.findings`, `rqx.plan`, `rqx.fix`,
`rqx.verify`, `rqx.baseline`, `rqx.suppress`, `rqx.config`, `rqx.dashboard`,
`rqx.disableRule`, `rqx.disableCategory`, `rqx.togglePatterns` and `rqx.toggleCodeLens`.
[docs/editor.md](../docs/editor.md) has what each takes and answers. Zed cannot bind them
because its extension API covers language servers, themes, MCP servers and debuggers and
nothing else.

**Quick fixes** are the real edits, verified exactly as `rqx fix` verifies them and tagged
`source.fixAll.rqx`, so an editor can apply every one of them on save. The rules whose fix
is a decision offer to explain themselves or turn the rule off instead.

The editor's **AI assistant** is a separate route: `rqx mcp` serves the same analysis over
MCP, so an agent in any of these editors can ask what a project's problems are rather than
reading every file to guess. It is configured per client rather than per extension —
[docs/mcp.md](../docs/mcp.md) has a snippet for each, JetBrains and Zed included.

## Shortcuts

One leader chord per editor, then a letter — one key claimed rather than eight, and the
same second key everywhere.

| | Leader |
|---|---|
| VS Code | `⌘⌥R` / `Ctrl+Alt+R` |
| JetBrains | `⌘⌥R` / `Ctrl+Alt+R` |
| Neovim, coc.nvim | `<leader>r` |
| Helix | `space r` |
| Sublime | `Ctrl+Alt+R` |
| Emacs | `C-c r` |

`s` scan · `c` check · `e` explain · `h` start here · `d` disable · `p` patterns ·
`l` lens · `o` panel · `w` dashboard · `f` fix the file · `x` waive · `v` verify.

Not every client binds every letter: the chord is what each editor's own setup snippet
writes, and a client with no command surface — Zed — binds none of them.

## The clients

| Path | What it is |
|---|---|
| [`vscode/`](vscode/README.md) | TypeScript extension: panel, status bar, bundled server |
| [`zed/`](zed/README.md) | Rust/WASM extension: server registration and binary download |

Neovim, Helix, Sublime, Emacs, Vim and Kate need no client code at all — see
[`docs/editor.md`](../docs/editor.md) for a block to paste.

## Getting the binary

Each ecosystem already knows how to install a program, so none of these writes its own
downloader: VS Code ships the server inside each platform build of the extension, Zed
fetches the checksummed release asset, Neovim users get `:MasonInstall rqx`, and everyone
else uses `brew`, `cargo install rqx`, or a release tarball. An explicitly configured path
always wins, then one on `PATH`, then whatever the client manages — a managed binary that
silently shadowed the installed one would let the editor and CI disagree.
