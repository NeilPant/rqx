# Rqx for VS Code

Inline diagnostics, hovers that explain *why* a finding matters, and a workspace
health score in the status bar — for TypeScript, Python, Go, Java and C++.

## Install

The extension drives the `rqx` binary; it does not bundle one.

```sh
cargo install --path crates/rqx-cli    # from a checkout
rqx --version                          # confirm it is on your PATH
```

Then either install the extension from the Marketplace, or build a `.vsix` from a
checkout:

```sh
cd editors/vscode
npm ci
npm run package        # writes rqx-<version>.vsix
code --install-extension rqx-*.vsix
```

If the binary is not on your `PATH`, point `rqx.path` at it.

## What it does

- **Diagnostics** on open and save. Not on every keystroke — a half-typed function
  is a work in progress, not a finding.
- **Hover** any flagged line for the measured fact, why it matters, and the named
  refactoring that fixes it.
- **Status bar** health score. Click it to rescan the workspace.
- **`Rqx: Scan Workspace`** runs the cross-file rules (unused exports, unused files,
  import cycles), which a single open file cannot answer.
- **`Rqx: Restart Server`** and **`Rqx: Show Server Log`** for when it misbehaves.

Editing `RQX.toml` restarts the server and rescans, so a threshold you change takes
effect without reloading the window.

## Settings

| Setting | Default | Meaning |
|---|---|---|
| `rqx.path` | `rqx` | Path to the binary |
| `rqx.scanOnStartup` | `true` | Scan the workspace when the extension activates |
| `rqx.configFile` | *(empty)* | An `RQX.toml` to use instead of the one rqx finds itself; relative paths resolve against the workspace root |

Rules and thresholds otherwise come from `RQX.toml`, resolved exactly as the CLI
resolves it — your own `~/.config/rqx/RQX.toml` underneath, the project's file over
it — so the editor and CI never disagree. See
[docs/configuration.md](../../docs/configuration.md).

## Troubleshooting

**"Rqx could not start"** — the binary is missing or not executable. Run
`rqx --version` in the same shell VS Code was launched from; if that works but the
extension still fails, set `rqx.path` to the absolute path.

**No diagnostics appear** — the file's language must be one of TypeScript, TSX,
Python, Go, Java, C or C++, and the file must be saved at least once. Check
**Rqx: Show Server Log** for what the server actually did.

**Findings you disagree with** — every rule can be switched off by name or by family
in `RQX.toml`, and [docs/rules/](../../docs/rules/README.md) explains what each one
measures before you do.
