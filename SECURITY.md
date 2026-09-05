# Security

rqx reads source files and runs `git`. It does not execute the code it analyses, and
it makes no network requests during a scan.

Two places take input that is not source code, and those are where to look:

- **`rqx mcp --http`** compares its bearer token in constant time and refuses to bind
  a public address without one.
- **Custom rules** are tree-sitter queries. They match against a syntax tree; nothing
  in a query executes.

Paths from an editor or an agent are confined to the project root before anything is
opened.

## Reporting

Open a [security advisory](https://github.com/jadhamwi21/rqx/security/advisories/new)
rather than a public issue. A description of the class of problem is enough to start;
please do not include a working exploit in the first message.
