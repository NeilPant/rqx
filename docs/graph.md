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

Cycles are listed first. Each ring is printed as a walk through its modules, in the
same form that `circular-dependency` reports it. The finding is anchored on the import
that opens the ring, not on the top of the file, so the edge to remove is the one that
is underlined.

## Formats

| `--format` | For |
|---|---|
| `text` | reading, the default |
| `dot` | `rqx graph --format dot \| dot -Tsvg > imports.svg` |
| `mermaid` | pasting into a design doc, a README or a wiki; GitHub renders it |
| `json` | anything else |

Cycle edges are red in `dot` and `mermaid`, and carry `"inCycle": true` in `json`.

## Narrowing it

A whole graph is hard to read beyond a few hundred modules.

```sh
rqx graph --cycles                      # only the edges that close a ring
rqx graph --focus src/billing/index.ts  # what it reaches, and what reaches it
rqx graph --focus src/api.ts --depth 1  # one hop, not two
```

`--focus` follows imports in both directions: the modules the file imports and the
modules that import it. The second set is not visible from the file itself, and both
are relevant before the file is changed.

## Breaking a cycle

The ring lists every module in it, so the decision is which edge to remove. Prefer an
edge whose direction is wrong for other reasons, such as a more general module
importing a more specific one, or a leaf module importing back into the module that
owns it. When `architecture` is configured in `rqx.yaml`, `layer-violation` already
reports those edges; see [configuration](configuration.md).

Two common ways to remove the edge are to move what the two modules share into a third
module that both can import, and to invert the weaker dependency behind an interface
that the owning module declares and the other module implements.
