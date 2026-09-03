# Rqx for VS Code

Inline diagnostics, hovers that explain *why* a finding matters, and a workspace
health score in the status bar.

## Requirements

The `rqx` binary on your `PATH`, or `rqx.path` pointing at it:

```
cargo install --path crates/rqx-cli
```

## What it does

- **Diagnostics** on open and save. Not on every keystroke — a half-typed function
  is a work in progress, not a finding.
- **Hover** any flagged line for the measured fact, why it matters, and the named
  refactoring that fixes it.
- **Status bar** health score. Click it to rescan the workspace.
- **`Rqx: Scan Workspace`** runs the cross-file rules (unused exports, unused files,
  import cycles), which a single open file cannot answer.

## Settings

| Setting | Default | Meaning |
|---|---|---|
| `rqx.path` | `rqx` | Path to the binary |
| `rqx.scanOnStartup` | `true` | Scan the workspace when the extension activates |

Rules and thresholds come from `rqx.toml` in the workspace root — the same file the
CLI reads, so the editor and CI never disagree.
