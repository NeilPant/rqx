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
letter. One key is claimed rather than a dozen, and the same second key means the same
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
| `⌘⌥R` `W` | Open the dashboard |
| `⌘⌥R` `F` | Fix this file — every proven edit in it, in one go |
| `⌘⌥R` `X` | Waive the finding under the cursor, with a reason |
| `⌘⌥R` `/` | Filter the Findings view |
| `⌘⌥R` `G` | Group the Findings view |
| `⌘⌥R` `V` | Verify that the refactoring landed |
| `⌘⌥R` `B` | Rank the work under a budget |

Every one is also in the command palette under **Rqx**, and rebindable in
**Keyboard Shortcuts**. The palette has the rest: fixing the whole workspace, taking a
baseline, exporting the report as JSON, SARIF, Markdown or JUnit, opening `RQX.toml`,
showing the effective settings, and setting the level.

## Fix on save

Fixes are tagged `source.fixAll.rqx`, so this applies every proven edit when you save:

```json
{ "editor.codeActionsOnSave": { "source.fixAll.rqx": "explicit" } }
```

Each one is verified the same way `rqx fix` verifies it — the file is re-parsed and
re-analysed before the edit is offered — so accepting one here is no less proven than
accepting one on the command line.

## What it does

- **Diagnostics** on open and save. Not on every keystroke — a half-typed function is a
  work in progress, not a finding.
- **Cross-file findings** without asking. The server scans the whole project after
  startup and after each save, so unused exports, unused files and import cycles appear
  for files you never opened.
- **Hover** any flagged line for the measured fact, why it matters, and the named
  refactoring that fixes it. The rule code links to the reference.
- **Quick fixes** that are the real edits, verified exactly as `rqx fix` verifies them
  and tagged `source.fixAll.rqx` so they can run on save. The rules whose fix is a
  decision offer to explain themselves or turn the rule off instead.
- **Four sidebar views** — Health (the score, the severity and grade splits, what to open
  first), Findings, Files and Rules.
- **Findings**, grouped by file, rule, severity or directory, and narrowed by any
  combination of severity, rule, family, path glob, fixability and baseline status. Fix,
  waive, explain or disable any of them from its context menu.
- **Files**, worst grade first, each expanding to its own findings, with blast radius and
  churn.
- **Rules** — the whole catalogue by family, with how often each one fired here.
- **The dashboard** in an editor tab: the treemap, the import graph with its cycles, the
  ranked plan, the trend and the activity feed. The same page `rqx serve` opens in a
  browser, started by the language server so there is only ever one.
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
| `rqx.dashboard.openIn` | `editor` | Whether the dashboard opens in an editor tab or your browser |
| `rqx.findings.groupBy` | `file` | How the Findings view groups what it shows |
| `rqx.findings.hideKnown` | `false` | Leave findings the baseline already recorded out of the Findings view |
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
