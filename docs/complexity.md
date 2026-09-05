# Complexity

Every rule with a threshold reports what crosses it and nothing else. That answers
*what is too complex* and never *compared to what* — so a threshold of 15 could be
flagging a third of your codebase or four functions in it, and the report looks the
same either way.

`rqx complexity` shows the measurements themselves.

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

That last line is the point. Half the functions score zero, ninety per cent score two
or less, and 137 of 37,410 are over the line — so the threshold is doing what it
should. A p90 above your threshold means the opposite: the rule is describing normal
code for this project, and either the code or the number needs to change.

## Asking a different question

```sh
rqx complexity --sort statements   # long rather than twisty
rqx complexity --sort nesting      # deep rather than long
rqx complexity --sort params       # wide signatures
rqx complexity --min 20            # skip everything under 20
```

The four measurements are independent. A function can be two hundred statements of
straight line with a complexity of 3, and that is a different problem from forty
statements nested five deep.

## By file and by directory

```sh
rqx complexity --by dir --top 5
```

```
    finql-api/src/modules/gamification/challenges    53    102     5   13  164 functions
    finql-app/src/features/learning/utils            52     84     4    5  59 functions
```

A directory reports the worst of what it holds, not the average. An average hides one
terrible function behind twenty ordinary ones, which is the opposite of useful when
you are scanning a list of directories deciding where to look.

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

The scale is square-rooted on purpose. Nearly every function in any codebase is
trivial, so a linear bar makes the first bucket the entire chart and every bucket
worth looking at a single character.

`--top 0` drops the table and leaves the distribution.

## JSON

`--json` gives one object per row — `name`, `at`, `complexity`, `statements`,
`nesting`, `parameters`, and `functions` for a file or directory row. The numbers are
the same ones the rules compare against, so a dashboard built on this and a gate built
on `cognitive_complexity` cannot disagree.
