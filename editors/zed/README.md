# Rqx for Zed

Diagnostics, hover, quick fixes and the per-file grade lens, for TypeScript, Python, Go,
Java and C++.

Install **Rqx** from Zed's extension registry. It fetches the matching `rqx` binary
itself, unless one is already on your `PATH` — that one wins, so the editor and CI never
run different versions.

```json
{
  "lsp": {
    "rqx": {
      "initialization_options": {
        "scanOnStartup": true,
        "codeLens": false,
        "patterns": false
      }
    }
  }
}
```

Turn `"code_lens": true` on in your settings to see each file's grade, blast radius and
churn above its first line.

## What Zed does not get

The commands — `rqx.scan`, `rqx.check`, `rqx.startHere` and the rest — are real
`workspace/executeCommand` handlers and work in every other editor rqx supports. A Zed
extension can contribute a language server, a theme, an MCP server or a debugger, and
nothing else, so there is nowhere to register them or bind them to a key. The workspace
is still scanned automatically after startup and after every save, so cross-file findings
appear without asking.
