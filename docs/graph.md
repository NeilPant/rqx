# The import graph

```sh
rqx graph
```

```
  Cycles  1 ring
    cyclea/cyclea.go → cycleb/cycleb.go → cyclec/cyclec.go → cyclea/cyclea.go

  Imports  26 edges from 4 modules

    cyclea/cyclea.go
      → cycleb/cycleb.go  closes a cycle
```

Cycles come first because they are the part worth acting on. A ring is printed as a
walk you can follow, which is also how `circular-dependency` reports it — the finding
anchors on the import that opens the ring rather than on the top of a file, so the
edge to delete is the thing underlined.

## Formats

| `--format` | For |
|---|---|
| `text` | reading, the default |
| `dot` | `rqx graph --format dot \| dot -Tsvg > imports.svg` |
| `mermaid` | pasting into a design doc, a README or a wiki — GitHub renders it |
| `json` | anything else |

Cycle edges are red in `dot` and `mermaid`, and carry `"inCycle": true` in `json`.

## Narrowing it

A whole graph is unreadable past a few hundred modules.

```sh
rqx graph --cycles                      # only the edges that close a ring
rqx graph --focus src/billing/index.ts  # what it reaches, and what reaches it
rqx graph --focus src/api.ts --depth 1  # one hop, not two
```

`--focus` follows imports in both directions on purpose. Someone about to change a
file wants to know what it depends on *and* who is watching, and only one of those is
visible from the file itself.

## Breaking a cycle

The ring names every module in it, so the question is which edge to cut. Prefer the
one whose direction is wrong on other grounds — the module that is more general
importing the one that is more specific, or a leaf importing back into the thing that
owns it. With `[architecture]` declared, `layer-violation` already names those edges;
see [architecture.md](configuration.md).

Whichever you pick, the usual answers are to move what the two modules share into a
third they can both import, or to invert the weaker dependency behind an interface
the owner declares and the other implements.
