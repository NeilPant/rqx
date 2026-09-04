# rqx for agents

`rqx mcp` is a Model Context Protocol server over stdio. An agent asks what a project's
problems are instead of reading every file to guess, and gets back the measured fact, the
reason it matters and the named refactoring that fixes it — the same four things a
finding carries everywhere else.

Answers are summaries with drill-down, never dumps. A scan of a large repository through
`--reporter json` is megabytes; an agent that pipes that into its context has nothing
left to fix anything with. So `scan` answers with health, grades and what to open first,
and the findings themselves are paged out of `findings`.

Every tool is read-only. Nothing rqx exposes to an agent writes a file, and the fix each
finding names is a refactoring for a person to make rather than an edit rqx could apply.

## Tools

| Tool | Answers |
|---|---|
| `scan` | Analyse the project: health, counts by rule and severity, grade distribution, what to open first |
| `findings` | The findings themselves, paged, narrowed by severity, rule, family or path glob |
| `plan` | The findings `rqx fix` will not touch, each with the source at its span, the metric that decided it and the named refactoring. Paged like `findings`. |
| `check` | Review the diff against a base revision and say whether the gate passes |
| `analyze` | Analyse source held in memory — an unsaved buffer — without touching the disk |
| `patterns` | Design patterns in use, half applied, or that would fit |
| `explain` | What one rule measures, why, and the refactoring it names. Also answers for a pattern |
| `rules` | Search the 60-rule catalogue by family, language or text |
| `config` | The settings this server will use, and which file each value came from |

`scan` runs a fresh scan. Every other project tool reads that scan, running one first if
nothing has been scanned yet — so a drill-down costs nothing and a cursor keeps pointing
at the findings it was issued against.

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

There is deliberately no findings array here. `ranked_below` is how many more files the
ranking holds — a count rather than an ellipsis, so an agent knows what it has not seen.

Every result also carries a text block: for `scan` and `check` it is the same Markdown
`--reporter markdown` writes, which is bounded at ten findings and a handful of tables.
The full JSON is in `structuredContent`, so a client that reads only one of the two still
gets a complete answer.

## Paging

`findings` takes `severity`, `rule`, `category`, `glob`, `limit` (20 by default, capped
at 100) and `cursor`. The filters narrow what is returned and never what was scanned, so
the health score and the gate do not move with a tool argument.

```json
{ "scan_id": 1, "matched": 71, "returned": 20, "next_cursor": "1:20", "findings": [ … ] }
```

Pass `next_cursor` back to get the next page; its absence means the end. A cursor names
the scan it came from, and one used after the project has been scanned again is refused
rather than answered from a different set of findings.

## Analysing a buffer

`analyze` takes `source`, and either a `filename` to read the language from or an
explicit `language`. The text is never written to disk, and the filename need not exist —
it is used to pick the language and to label the findings.

One buffer answers 52 of the 60 rules. Unused exports, unused files, import cycles,
duplication and the two co-change rules need the whole project, and the result says so,
because an agent that gets no `unused-export` back would otherwise read the silence as a
clean bill of health.

## Over HTTP

```sh
rqx mcp --http 127.0.0.1:8927
```

Serves Streamable HTTP at `/mcp` for clients that cannot spawn a process — hosted
connectors, or one endpoint shared by a team. There is no default port: a magic number is
one more thing to collide with.

**A remote bind needs a token.** Anyone who can reach the port can read the source of
every file rqx parses, so a non-loopback address with no token is refused at startup
rather than warned about. The token comes from `RQX_MCP_TOKEN` and never from a flag,
because a flag is visible in `ps`:

```sh
RQX_MCP_TOKEN=$(openssl rand -hex 32) rqx mcp --http 0.0.0.0:8927
```

Clients then send `Authorization: Bearer <token>`. A request carrying an `Origin` header
is refused whatever its token: an MCP client sends none, and a browser doing DNS
rebinding does.

Every `path` an agent passes is resolved against the directory the server was started on,
canonicalised, and refused if it escapes — symlinks included, since a link inside the
project pointing at `/etc` only reveals itself once resolved. That holds on stdio too: a
local agent asking to scan `/etc` is the same leak as a remote one.

## Claude Code

```sh
claude mcp add rqx -- rqx mcp
```

Or check a `.mcp.json` into the repository so everyone on the project gets it:

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
2026.1. **Settings → Tools → AI Assistant → Model Context Protocol → Add**, then paste:

```json
{ "mcpServers": { "rqx": { "command": "rqx", "args": ["mcp"] } } }
```

The IDE launches the command itself rather than through a shell, so `rqx` has to be on
the PATH that the IDE sees — which on macOS is not the one your terminal has when the IDE
was started from Spotlight. Give the absolute path from `which rqx` if the server does
not start.

## Zed

In `settings.json`:

```json
{ "context_servers": { "rqx": { "source": "custom", "command": "rqx", "args": ["mcp"] } } }
```

## Anything else

Any client that speaks MCP over stdio works: run `rqx mcp` with the project root as the
working directory, or pass the root as a positional argument. Messages are
newline-delimited JSON-RPC, and nothing but JSON-RPC reaches stdout — the progress bar
writes to stderr and silences itself when stderr is not a terminal.

For a client that only speaks HTTP, point it at the address `--http` was given:

```json
{ "type": "http", "url": "http://127.0.0.1:8927/mcp",
  "headers": { "Authorization": "Bearer YOUR_TOKEN" } }
```
