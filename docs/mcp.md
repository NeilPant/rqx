# rqx for agents

`rqx mcp` is a Model Context Protocol server over stdio. An agent queries it for a
project's findings. Each answer carries the measured fact, the reason it matters and
the named refactoring that addresses it, the same elements a finding carries in every
other reporter.

Answers are summaries with drill-down. A scan of a large repository through
`--reporter json` can be megabytes, which is too large to place in an agent's context.
`scan` therefore returns health, grades and the files to open first, and the findings
themselves are paged out of `findings`.

Every tool is read-only. No tool writes a file. The fix each finding names is a
refactoring for a person to make, not an edit that rqx applies. The mechanical fixes
that `rqx fix` applies are returned by `fix_preview` as diffs and are not written.

## Tools

| Tool | Answers |
|---|---|
| `scan` | Analyse the project: health, counts by rule and severity, grade distribution, what to open first |
| `findings` | The findings themselves, paged, narrowed by severity, rule, family or path glob |
| `plan` | The findings `rqx fix` will not touch, each with the source at its span, the metric that decided it and the named refactoring. Paged like `findings`. |
| `fix_preview` | The edits `rqx fix` would make to one file, as a unified diff, with the findings they resolve |
| `verify` | Compare the working tree against a base revision and grade what the diff did to each finding. Propose, grade, retry. |
| `check` | Review the diff against a base revision and say whether the gate passes |
| `analyze` | Analyse source held in memory, such as an unsaved buffer, without writing to disk |
| `patterns` | Design patterns in use, half applied, or that would fit |
| `explain` | What one rule measures, why, and the refactoring it names. Also answers for a pattern |
| `rules` | Search the 195-rule catalogue by family, language or text |
| `config` | The settings this server will use, and which file each value came from |
| `complexity` | Every function's cognitive complexity, statements, nesting and parameters, with the project's percentiles |
| `health` | Where the score comes from, by family: findings, share of the weighted debt, and the score without each |
| `perf` | Every function that grows faster than its input, ranked by cost against reach, with the loop order, the round trips inside its loops and the named fix |
| `graph` | Every import edge, and the cycles as ordered rings; each edge states whether it closes a cycle |
| `recommend` | Whether the thresholds fit, which refactoring answers the most, and what to read first |

`scan` runs a fresh scan. Every other project tool reads that scan, and runs one first
if nothing has been scanned yet. A drill-down therefore does not rescan, and a cursor
keeps referring to the findings it was issued against.

## What a scan answers

```json
{
  "scan_id": 1,
  "scanned": "/home/you/project",
  "health": 62,
  "files_scanned": 41,
  "code_lines": 5120,
  "elapsed_ms": 840,
  "findings": {
    "total": 137,
    "by_severity": { "error": 12, "warning": 71, "info": 54 },
    "by_rule": { "long-function": 22, "magic-number": 19, "unused-export": 11 }
  },
  "grades": { "critical": 3, "poor": 6, "fair": 11, "clean": 12, "pristine": 9 },
  "start_here": [
    { "file": "src/router.ts", "grade": "critical", "findings": 19, "importers": 14, "commits": 31 }
  ],
  "ranked_below": 31,
  "advice": 26,
  "skipped": []
}
```

The result contains no findings array. `ranked_below` is the number of further files in
the ranking that are not listed.

Every result also carries a text block. For `scan` and `check` it is the Markdown that
`--reporter markdown` writes, limited to ten findings and a few tables. The full JSON
is in `structuredContent`, so a client that reads only one of the two receives a
complete answer.

## Paging

`findings` takes `severity`, `rule`, `category`, `glob`, `limit` (20 by default, at most
100) and `cursor`. The filters narrow what is returned, not what was scanned, so tool
arguments do not change the health score or the gate.

```json
{ "scan_id": 1, "matched": 71, "returned": 20, "next_cursor": "1:20", "findings": [ … ] }
```

Pass `next_cursor` back to get the next page. When it is absent, there are no more
pages. A cursor records the scan it came from, and a cursor used after the project has
been scanned again is refused instead of being answered from a different set of
findings.

## Previewing a fix

`fix_preview` takes `file`, relative to the server's root, and optionally `rule`, a rule
slug. It runs the same planning as `rqx fix` on that file and returns the result
without writing anything. Each edit is applied to a copy of the file and the copy is
analysed again. An edit is kept only if its finding is gone and no new finding
appears, so every edit in the diff has been verified. `rule` narrows the preview to
that rule's fixes. A rule with no mechanical fix, or an unknown rule, is refused.
Which fixes `rqx fix` may apply is still set by the `fix` section of the configuration.

```json
{
  "file": "src/orders.ts",
  "rule": null,
  "fixable": true,
  "diff": "--- a/src/orders.ts\n+++ b/src/orders.ts\n@@ -10,7 +10,6 @@\n …",
  "edits": [
    { "rule": "unreachable-code", "kind": "deletion", "line": 12, "column": 3,
      "start": 214, "end": 241, "removed": "console.log('dead')", "replacement": "" }
  ],
  "resolves": [ { "rule": "unreachable-code", "file": "src/orders.ts", "line": 12 } ],
  "withheld": null
}
```

`diff` is a unified diff that `git apply` and `patch` accept. `resolves` holds the
findings the edits answer, in the shape `findings` returns. When no edit is proven,
`fixable` is false and `diff` is empty. If edits were planned and then withheld, for
example because applying them would introduce a finding, `withheld` gives the number
of edits and the reason.

The preview is the first round of fixing. `rqx fix --write` repeats until a round
finds nothing, up to `fix.max_rounds`, so a second preview taken after applying the diff
can show further edits. After applying a diff, call `verify` to confirm the findings
are gone.

## Analysing a buffer

`analyze` takes `source`, and either a `filename` to read the language from or an
explicit `language`. The text is not written to disk, and the file named by `filename`
does not need to exist. The name selects the language and labels the findings.

A single buffer cannot be checked against every rule. Unused exports, unused files,
import cycles, duplication and the two co-change rules need the whole project. The
result states this, so that the absence of an `unused-export` finding is not
read as a clean result.

## Instructing a coding agent

`rqx setup agent` prints the instructions that make a coding agent run rqx before it
finishes a task. `--for` selects the agent: `claude`, `codex`, `cursor` or `generic`,
which is the default. Without `--write` it only prints.

```sh
rqx setup agent --for claude          # print
rqx setup agent --for claude --write  # add to the project's files
```

The instructions tell the agent to stage its changes and run `rqx check --staged`, or
`rqx check --base <branch>` for committed work, then to read each finding, apply the
named refactoring, and confirm it with `rqx verify --base <branch> --expect
<rule>@<file>`. They also point at the MCP tools when the server is configured.

| `--for` | Instructions file | Also |
|---|---|---|
| `claude` | `CLAUDE.md` | A Stop hook in `.claude/settings.json` |
| `codex`, `cursor`, `generic` | `AGENTS.md` | |

`--write` never replaces a file. The instructions go between `<!-- rqx:begin -->` and
`<!-- rqx:end -->`. The first run appends that block to the file, and a later run
replaces only the text between the markers. A file whose markers are missing one end,
repeated or in the wrong order is left as it is and the command fails.

For `claude`, `--write` also adds a Stop hook to `.claude/settings.json`:

```json
{
  "hooks": {
    "Stop": [
      { "hooks": [{ "type": "command", "command": "rqx check --staged >&2; [ $? -ne 1 ] || exit 2" }] }
    ]
  }
}
```

Claude Code holds the agent back when a Stop hook exits with 2 and shows it the error
output. The command exits 2 only when the gate fails, which is exit code 1 of
`rqx check`, and sends the report to the error output. Any other result, including
rqx not being installed, lets the agent finish. The command needs a POSIX shell.

The file is parsed as JSON, and the hook is added next to every other key and every
other Stop hook. The file is rewritten as indented JSON with its keys in alphabetical
order. If a Stop hook running `rqx check --staged` is already there, the file is not
changed. A file that is not valid JSON, or whose `hooks` or `Stop` value is not an
object or list, is left as it is and the command fails.

`rqx check --staged` reads the index, so the hook reports on changes the agent has
staged with `git add`. Unstaged edits are not part of the verdict.

## Over HTTP

```sh
rqx mcp --http 127.0.0.1:8927
```

Serves Streamable HTTP at `/mcp`, for clients that cannot spawn a process, such as
hosted connectors, or for one endpoint shared by a team. There is no default port.

A non-loopback bind requires a token. Anyone who can reach the port can read the
source of every file rqx parses, so a non-loopback address without a token is refused
at startup. The token is read from `RQX_MCP_TOKEN` and not from a flag, because a flag
is visible in `ps`:

```sh
RQX_MCP_TOKEN=$(openssl rand -hex 32) rqx mcp --http 0.0.0.0:8927
```

Clients send `Authorization: Bearer <token>`. A request carrying an `Origin` header is
refused regardless of its token. MCP clients do not send `Origin`, and a browser
performing DNS rebinding does.

Every `path` an agent passes is resolved against the directory the server was started
in and canonicalised. A path that resolves outside that directory is refused, including
through a symlink, because a link inside the project that points at `/etc` is visible
only after resolution. The same check applies on stdio.

## Claude Code

```sh
claude mcp add rqx -- rqx mcp
```

Alternatively, commit a `.mcp.json` to the repository so that everyone on the project
uses the same configuration:

```json
{ "mcpServers": { "rqx": { "command": "rqx", "args": ["mcp"] } } }
```

## Cursor and Windsurf

`.cursor/mcp.json` in the project, or the global `~/.cursor/mcp.json`:

```json
{ "mcpServers": { "rqx": { "command": "rqx", "args": ["mcp"] } } }
```

## VS Code

`.vscode/mcp.json`:

```json
{ "servers": { "rqx": { "type": "stdio", "command": "rqx", "args": ["mcp"] } } }
```

## JetBrains

IntelliJ IDEA, PyCharm, WebStorm, GoLand, CLion, Rider, PhpStorm and RubyMine, from
2026.1. Open Settings → Tools → AI Assistant → Model Context Protocol → Add, then paste:

```json
{ "mcpServers": { "rqx": { "command": "rqx", "args": ["mcp"] } } }
```

The IDE launches the command directly, without a shell, so `rqx` must be on the `PATH`
that the IDE sees. On macOS, an IDE started from Spotlight does not inherit the
terminal's `PATH`. If the server does not start, use the absolute path printed by
`which rqx`.

## Zed

In `settings.json`:

```json
{ "context_servers": { "rqx": { "source": "custom", "command": "rqx", "args": ["mcp"] } } }
```

## Anything else

Any client that speaks MCP over stdio can use the server. Run `rqx mcp` with the project
root as the working directory, or pass the root as a positional argument. Messages are
newline-delimited JSON-RPC, and only JSON-RPC is written to stdout. The progress bar
is written to stderr and is disabled when stderr is not a terminal.

For a client that only speaks HTTP, point it at the address `--http` was given:

```json
{ "type": "http", "url": "http://127.0.0.1:8927/mcp",
  "headers": { "Authorization": "Bearer YOUR_TOKEN" } }
```
