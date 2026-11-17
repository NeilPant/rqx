# Complexity

A rule with a threshold reports only the findings that exceed it. The report does not
show how the rest of the codebase measures against that threshold, so the same
threshold of 15 can flag a third of a codebase or four functions, and the output looks
the same in both cases.

`rqx complexity` prints the measurements themselves.

```sh
rqx complexity --top 8
```

```
  Complexity  37410 functions measured

                        cplx  stmts  nest args  where
    submit                53    102     5    3  …/challenges/challenges.service.ts:658
    projectMarkdown       52     84     4    1  …/utils/markdown-projection.ts:30
    createOrderInner      46    112     3    2  …/trading/orders/order.service.ts:95

    p50 0 · p90 2 · p99 9 · 137 of 37410 over the threshold of 15
```

The last line summarises the distribution: the 50th, 90th and 99th percentiles, and
the number of functions over the threshold. In this example the median is 0, 90 per
cent of functions score 2 or less, and 137 of 37,410 functions are over the threshold,
so the threshold selects a small tail. If p90 is above the threshold, the rule flags
typical code for the project, and either the code or the threshold needs to change.

## Asking a different question

```sh
rqx complexity --sort statements   # long rather than twisty
rqx complexity --sort nesting      # deep rather than long
rqx complexity --sort params       # wide signatures
rqx complexity --min 20            # skip everything under 20
```

The four measurements are independent. A function can have two hundred statements of
straight-line code and a complexity of 3, which is a different problem from forty
statements nested five levels deep.

## By file and by directory

```sh
rqx complexity --by dir --top 5
```

```
    finql-api/src/modules/gamification/challenges    53    102     5   13  164 functions
    finql-app/src/features/learning/utils            52     84     4    5  59 functions
```

`--by` selects the row type: `function` (the default), `file` or `dir`. A directory row
reports the worst value it holds, not the average, because an average hides a single
high-scoring function among many ordinary ones.

## The shape of it

```sh
rqx complexity --histogram --top 0
```

```
    how many functions score what, on a square-root scale
       0–2    ████████████████████████████████████████ 34962
       3–5    █████████ 1611
       6–8    █████ 478
      15–17   ██ 51
      51–53   █ 2
```

The histogram uses a square-root scale. Most functions in a codebase are trivial, and
on a linear scale the first bucket would take up the whole chart and every other
bucket would be a single character wide.

`--top 0` omits the table and prints only the distribution.

## JSON

`--json` prints one object per row with the fields `name`, `at`, `complexity`,
`statements`, `nesting` and `parameters`, plus `functions` for a file or directory
row. The values are the same ones the rules compare against, so a chart built from
this output and a gate built on `cognitive_complexity` use the same numbers.
