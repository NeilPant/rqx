# Security

rqx reads source files and runs `git`. It does not execute the code it analyses. A scan
makes no network request except the once-a-day update check, which sends no project
data and can be turned off (see "Network access" in the README).

Two components accept input that is not source code:

- `rqx mcp --http` compares its bearer token in constant time and refuses to bind a
  public address without one.
- Custom rules are tree-sitter queries. A query matches against a syntax tree and
  does not execute anything.

Paths received from an editor or an agent are confined to the project root before any
file is opened.

## Reporting

Report vulnerabilities through a
[private security advisory](https://github.com/jadhamwi21/rqx/security/advisories/new)
instead of a public issue. The first message should describe the class of problem.
It should not include a working exploit.

If the advisory page is not available to you, email the maintainer at
<jadhamwi4@gmail.com> with the same information. Do not open a public issue for
either route.
