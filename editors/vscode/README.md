# Rqx for VS Code

Inline diagnostics, hovers that explain *why* a finding matters, a panel with the health
score and what to open first — for TypeScript, Python, Go, Java and C++.

## Install

Install from the Marketplace or Open VSX. There is nothing else to do: the build for your
platform carries the matching `rqx` server inside it, so the language server starts on
first activation with no toolchain, no `PATH` entry and no download. A binary already on
your `PATH`, or one named by `rqx.path`, is preferred over the shipped one — set
`rqx.path` to an empty string to always use the shipped one.

On a platform with no published build — Alpine, 32-bit ARM — the universal package
installs instead and fetches the checksummed release asset into extension storage on
first run.

To build a `.vsix` from a checkout instead:

```sh
cargo build --release --bin rqx
cd editors/vscode
mkdir -p server && cp ../../target/release/rqx server/
npm ci
npm run package        # writes rqx-<version>.vsix
code --install-extension rqx-*.vsix
```

Skip the `server/` copy and the extension falls back to `PATH` or a download, exactly as
the universal package does.

## Shortcuts

Everything is behind one chord — `cmd+alt+r` on macOS, `ctrl+alt+r` elsewhere — then a
letter. One key is claimed rather than eight, and the same second key means the same
thing in every editor rqx supports.

| Keys | Does |
|---|---|
| `⌘⌥R` `S` | Scan the workspace |
| `⌘⌥R` `C` | Review the diff against `origin/HEAD` and report the gate |
| `⌘⌥R` `E` | Explain the rule under the cursor, rendered beside the code |
| `⌘⌥R` `H` | Open what matters first |
| `⌘⌥R` `D` | Disable the rule under the cursor in `RQX.toml` |
| `⌘⌥R` `P` | Toggle design-pattern advice |
| `⌘⌥R` `L` | Toggle the code lens |
| `⌘⌥R` `O` | Open the panel |

Every one is also in the command palette under **Rqx**, and rebindable in
**Keyboard Shortcuts**.

## What it does

- **Diagnostics** on open and save. Not on every keystroke — a half-typed function is a
  work in progress, not a finding.
- **Cross-file findings** without asking. The server scans the whole project after
  startup and after each save, so unused exports, unused files and import cycles appear
  for files you never opened.
- **Hover** any flagged line for the measured fact, why it matters, and the named
  refactoring that fixes it. The rule code links to the reference.
- **Quick fixes** that switch a rule or a whole family off in `RQX.toml`. None of them
  rewrites your code: every fix rqx names is a refactoring, not an edit it could apply
  for you.
- **The panel** — health, the severity and grade distributions, a ranked list of what to
  open, and the searchable catalogue of all 60 rules with what each measures and where it
  comes from.
- **Status bar** health score. Click it to open the panel.
- **Code lens** (off by default) putting each file's grade, blast radius and churn above
  its first line.

Editing `RQX.toml` reloads the settings in place, so a threshold you change takes effect
without restarting anything.

## Settings

| Setting | Default | Meaning |
|---|---|---|
| `rqx.path` | `rqx` | Path to the binary; empty to always use the managed one |
| `rqx.scanOnStartup` | `true` | Scan the workspace when the extension activates |
| `rqx.configFile` | *(empty)* | An `RQX.toml` to use instead of the one rqx finds itself |
| `rqx.codeLens` | `false` | Show the per-file grade lens |
| `rqx.patterns` | `false` | Report half-applied design patterns as hints |

Rules and thresholds otherwise come from `RQX.toml`, resolved exactly as the CLI resolves
it — your own `~/.config/rqx/RQX.toml` underneath, the project's file over it — so the
editor and CI never disagree. See [docs/configuration.md](../../docs/configuration.md).

## Troubleshooting

**"Rqx could not find or download a binary"** — no published build matches your platform,
or the download failed. Install it yourself (`cargo install rqx`) and set `rqx.path`.
**Rqx: Show Server Log** says which step failed.

**No diagnostics appear** — the file's language must be one of TypeScript, TSX, Python,
Go, Java or C++, and the file must exist on disk. Check **Rqx: Show Server Log**.

**Findings you disagree with** — press `⌘⌥R D` on the line, or switch the rule off by
name or by family in `RQX.toml`. [docs/rules/](../../docs/rules/README.md) explains what
each one measures before you do.
