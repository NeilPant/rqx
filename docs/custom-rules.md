# Rules you write

A project can define rules that rqx does not ship, such as "do not import
`legacyClient` from new code", "no `sleep` outside `tools/`" or "every handler calls
`audit()`". The tree-sitter grammars are already in the binary, and a `rule` entry in
`rqx.yaml` supplies the query.

```yaml
rule:
  - id: no-legacy-client
    languages: [typescript]
    query: |
      ((call_expression function: (member_expression object: (identifier) @obj)) @site
        (#eq? @obj "legacyClient"))
    message: legacyClient is retired; call ApiClient
    severity: error
    family: coupling
    hint: Replace with ApiClient and delete the import
```

```
   ● no-legacy-client  5:10  error
     3 │
     4 │ export function fetchUser(id: string) {
     5 │   return legacyClient.get(`/users/${id}`);
       │          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
     6 │ }
     legacyClient is retired; call ApiClient
     Fix:  Replace with ApiClient and delete the import
```

A custom rule is handled like a shipped rule. It counts towards the health score,
appears in the editor, is written to SARIF with its own descriptor, and is visible to
an agent over MCP. It can be disabled with `rules.disabled: [no-legacy-client]` in
`rqx.yaml`, and a waiver can name it.

## Fields

| Field | Description |
|---|---|
| `id` | Required. The name used everywhere the rule is referenced. Cannot be the name of a rule rqx already ships. |
| `languages` | Required. Any of `typescript`, `javascript`, `python`, `go`, `java`, `cpp`. |
| `query` | Required. A [tree-sitter query](https://tree-sitter.github.io/tree-sitter/using-parsers/queries/1-syntax.html). |
| `message` | Required. The text shown to the reader. It should state what is wrong rather than what the query matched. |
| `family` | Required. Any family slug: `size`, `abstraction`, `change`, `waste`, `coupling`, `flow`, `naming`, `safety`, `tests`. |
| `severity` | `info`, `warning` or `error`. Defaults to `warning`. |
| `confidence` | `low`, `heuristic` or `verified`. Defaults to `verified`, because a query either matches or does not. |
| `hint` | Optional. What to do instead. |

## Where it reports

The `@site` capture is where the finding is anchored. Without one, the first capture
in the match is used, which is usually the outermost pattern.

```
((call_expression function: (member_expression object: (identifier) @obj)) @site
  (#eq? @obj "legacyClient"))
```

Here `@site` is the whole call, so the whole call is underlined. `@obj` exists only so
that the `#eq?` predicate has something to test.

## Trying one before you write it

```sh
rqx query typescript '((call_expression function: (member_expression object: (identifier) @obj)) @site (#eq? @obj "legacyClient"))' --captures
```

```
  api.ts:5:10  legacyClient.get(`/users/${id}`)
      @site  call_expression  legacyClient.get(`/users/${id}`)
      @obj  identifier  legacyClient

  1 match
```

`--captures` prints every capture of every match, which shows what would be underlined
before the rule is added to the configuration.

A query that does not compile is reported as a configuration error that names and
locates the rule. The rule is not silently skipped:

```
Error: `no-legacy-client` will not compile against typescript: Unexpected EOF at row 2, column 0
```

## Cookbook

A call to a banned function:

```
((call_expression function: (identifier) @site) (#eq? @site "eval"))
```

A method on a banned object, in Python:

```
((call attribute: (attribute object: (identifier) @obj)) @site (#eq? @obj "requests"))
```

An import of a package:

```
((import_statement source: (string) @from) @site (#match? @from "lodash"))
```

A field with a name pattern, in Java:

```
((field_declaration declarator: (variable_declarator name: (identifier) @name)) @site
  (#match? @name "^m[A-Z]"))
```

A Go function returning more than three values:

```
(function_declaration result: (parameter_list (parameter_declaration) (parameter_declaration)
  (parameter_declaration) (parameter_declaration))) @site
```

Anything inside a `catch` that is not a rethrow, in TypeScript:

```
((catch_clause body: (statement_block (expression_statement) @site)))
```

rqx supports the tree-sitter predicates `#eq?`, `#not-eq?`, `#match?`, `#not-match?`
and `#any-of?`.

## Grammar node names

Node names are defined by each language's grammar, not by rqx, and they differ between
languages: a call is `call_expression` in TypeScript and `call` in Python. To find the
right name, write a loose query and narrow it with `rqx query --captures` until it
matches only the intended nodes.

## What a custom rule cannot do

A custom rule sees the syntax tree of one file. It cannot follow an import, resolve a
type, or determine what a name refers to elsewhere. A custom rule has no autofix: rqx applies a fix only where a rule has
exactly one correct result, and a query cannot specify that result.
