# Rules you write

Every organisation has rules rqx will never ship. *Don't import `legacyClient` from
new code.* *No `sleep` outside `tools/`.* *Every handler calls `audit()`.* The
grammars are already in the binary; a `rule` entry is what lets you ask.

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

It behaves like any other rule from there: it counts towards the health score, it
shows in the editor, it lands in SARIF with its own descriptor, an agent sees it over
MCP, `[rules] disabled = ["no-legacy-client"]` switches it off, and a waiver can name
it.

## Fields

| Field | |
|---|---|
| `id` | Required. The name everything calls it. Cannot be a rule rqx already ships. |
| `languages` | Required. Any of `typescript`, `python`, `go`, `java`, `cpp`. |
| `query` | Required. A [tree-sitter query](https://tree-sitter.github.io/tree-sitter/using-parsers/queries/1-syntax.html). |
| `message` | Required. What the reader sees. Say what is wrong, not what the query matched. |
| `family` | Required. Any family slug: `size`, `abstraction`, `change`, `waste`, `coupling`, `flow`, `naming`, `safety`, `tests`. |
| `severity` | `info`, `warning` or `error`. Defaults to `warning`. |
| `confidence` | `low`, `heuristic` or `verified`. Defaults to `verified` — a query either matched or it did not. |
| `hint` | Optional. What to do instead. |

## Where it reports

The `@site` capture is where the finding is anchored. Without one, the first capture
in the match is used, which is usually the outermost pattern.

```
((call_expression function: (member_expression object: (identifier) @obj)) @site
  (#eq? @obj "legacyClient"))
```

`@site` is the whole call, so that is what gets underlined; `@obj` only exists so the
`#eq?` predicate has something to test.

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

`--captures` shows every capture of every match, so you can see exactly what would be
underlined before committing to it.

A query that does not compile is a configuration error, named and located, rather
than a rule that silently never fires:

```
Error: `no-legacy-client` will not compile against typescript: Unexpected EOF at row 2, column 0
```

## Cookbook

**A call to a banned function**

```
((call_expression function: (identifier) @site) (#eq? @site "eval"))
```

**A method on a banned object, in Python**

```
((call attribute: (attribute object: (identifier) @obj)) @site (#eq? @obj "requests"))
```

**An import of a package**

```
((import_statement source: (string) @from) @site (#match? @from "lodash"))
```

**A field with a name pattern, in Java**

```
((field_declaration declarator: (variable_declarator name: (identifier) @name)) @site
  (#match? @name "^m[A-Z]"))
```

**A Go function returning more than three values**

```
(function_declaration result: (parameter_list (parameter_declaration) (parameter_declaration)
  (parameter_declaration) (parameter_declaration))) @site
```

**Anything inside a `catch` that is not a rethrow, in TypeScript**

```
((catch_clause body: (statement_block (expression_statement) @site)))
```

Predicates rqx supports are the ones tree-sitter does: `#eq?`, `#not-eq?`, `#match?`,
`#not-match?`, `#any-of?`.

## Grammar node names

Node names come from the grammar, not from rqx, and they differ per language — a
TypeScript call is `call_expression`, a Python one is `call`. The fastest way to find
the right name is to write a loose query and narrow it with `rqx query --captures`
until it matches only what you meant.

## What a custom rule cannot do

It sees one file's syntax tree. It cannot follow an import, resolve a type, or know
what a name refers to somewhere else — the same boundary every rule rqx ships works
inside. There is no autofix: rqx applies a fix only where a rule has exactly one
right answer, and a query cannot say what that answer is.
