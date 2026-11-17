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
refactoring for a person to make, not an edit that rqx applies.

## Tools

| Tool | Answers |
|---|---|
| `scan` | Analyse the project: health, counts by rule and severity, grade distribution, what to open first |
| `findings` | The findings themselves, paged, narrowed by severity, rule, family or path glob |
| `plan` | The findings `rqx fix` will not touch, each with the source at its span, the metric that decided it and the named refactoring. Paged like `findings`. |
| `verify` | Compare the working tree against a base revision and grade what the diff did to each finding. Propose, grade, retry. |
| `check` | Review the diff against a base revision and say whether the gate passes |
| `analyze` | Analyse source held in memory, such as an unsaved buffer, without writing to disk |
| `patterns` | Design patterns in use, half applied, or that would fit |
| `explain` | What one rule measures, why, and the refactoring it names. Also answers for a pattern |
| `rules` | Search the 185-rule catalogue by family, language or text |
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

## Analysing a buffer

`analyze` takes `source`, and either a `filename` to read the language from or an
explicit `language`. The text is not written to disk, and the file named by `filename`
does not need to exist. The name selects the language and labels the findings.

A single buffer cannot be checked against every rule. Unused exports, unused files,
import cycles, duplication and the two co-change rules need the whole project. The
result states this, so that the absence of an `unused-export` finding is not
read as a clean result.

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
