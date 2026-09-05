# Changelog

Notable changes. Dates are when the change landed on `main`.

## Unreleased

### Added

- **Baseline and ratchet.** `rqx baseline` records the debt a project already has;
  `[gate] baseline` counts only findings it does not hold, and `[gate] ratchet`
  refuses to let the total grow. [docs](docs/baseline.md)
- **Waivers.** `// rqx-ignore <rule>: <reason>` waives a finding, and the reason is
  not optional — without one the finding stands and `unjustified-suppression`
  reports the comment. `rqx suppressions` audits them. [docs](docs/suppressions.md)
- **Rules you write.** `[[rule]]` blocks holding tree-sitter queries, with
  `rqx query` to try one first. [docs](docs/custom-rules.md)
- **Policy.** `policy = "…"` reads a config from outside the repository, layered
  under the project so a service may still tighten it, with `[locked] keys` naming
  what it may not change. [docs](docs/policy.md)
- **`rqx config set/unset/files`** — change a setting from the command line, in the
  right file, without losing the comments.
- **`rqx complexity`**, **`rqx health`**, **`rqx slop`**, **`rqx recommend`**,
  **`rqx graph`**, **`rqx cache`**, **`rqx completions`**.
- **The `noise` family** — five rules for text that costs a reader and buys nothing.
- `--stats` for counts without findings, `--timings` for where a scan spent itself.
- Four more MCP tools: `complexity`, `health`, `graph`, `recommend`.

### Removed

- **The browser dashboard.** `rqx serve`, the `html` reporter, the `rqx.dashboard`
  LSP command and the editor buttons that opened it are all gone. A second surface
  cost more to keep honest than it returned, and every table it drew is a table the
  terminal interface draws. `rqx` on its own is the interface now.
- `rqx tui` — the interface opens on `rqx` and on `rqx scan` in a terminal; nothing
  is left for the subcommand to name.

### Changed

- **The cache is on by default.** It was off because a warm run was no faster than a
  cold one; that was measured while the collector waited for `git log` before
  draining its channel. With that fixed, a warm scan of 3,440 files costs 298ms
  against 2,023ms cold.
- **A cycle is reported as a ring** — `a.ts → b.ts → c.ts → a.ts` — anchored on the
  import that opens it rather than on byte zero of whichever file sorted first.
- `verify` recognises an Extract Method that stayed in the file it came from.

### Fixed

- **Findings could differ between runs** on any project with more than
  `max_findings` of them. The heap that chose which to keep ranked only on severity,
  confidence and rule, so ties were broken by whichever worker finished first.
- **`Identity` is FNV-1a rather than `DefaultHasher`.** Its values are persisted in
  GitLab reports and now in baselines, and the standard library promises nothing
  across versions. GitLab fingerprints change once.
- Release URLs, the VS Code extension's download base and the Action's binary base
  all named a repository that does not exist.
- The VS Code extension registered six commands the language client already
  registers from the server, so it failed to start at all.
