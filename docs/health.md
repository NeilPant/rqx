# Where the score comes from

The health score is one number, and one number cannot be argued with. `rqx health`
is the same arithmetic broken into buckets.

```sh
rqx health
```

```
  Health  0/100

               found    debt  worst of it
    safety       369     67%  output-parameter
    waste        261     28%  empty-catch, unreachable-code
    size          24      3%  many-exits
    coupling       9      0%  message-chain
```

`found` is how many findings. `debt` is what they weigh in the score's own
arithmetic — severity times reach, as a share of the whole. The two disagree often,
and the disagreement is the point: 24 `size` findings carry more weight here than 9
`coupling` ones, and elsewhere sixteen coupling findings carried a third of what
seventeen flow findings did.

## Two things this tells you

**Where the score went.** Sorted by debt, so the top row is what to fix if you are
fixing one thing.

**When the score has stopped being useful.** Past a certain density the curve bottoms
out and every bucket leaves it at zero. When that happens rqx says so and drops the
`without` column rather than printing `0 (+0)` on every row:

```
    the score stays at 0 whichever single bucket goes, so `debt` is what separates
    them: the weight each carries in the same arithmetic
```

A project in that state cannot be steered by the score. Set a
[baseline](baseline.md) and gate on what changes instead.

## When there is room to move

On a project that is not saturated, a third column appears:

```
               found    debt   without  worst of it
    waste           12     41%   78 (+9)  unused-export, unused-file
    size             4     22%   74 (+5)  long-function
```

`without` is the score with that bucket gone — recomputed, not apportioned. The curve
is exponential, so the gains do not add up: removing waste and size together buys
less than 9 + 5.

## Grouping

```sh
rqx health --by rule    # which rule costs most
rqx health --by dir     # which part of the tree
rqx health --by file    # which file
```

`--by rule` is the one to reach for when a family looks expensive and you want to
know whether it is one rule firing everywhere or a spread.

`--json` gives one object per bucket, with `debtShare`, `healthWithout` and `gain`.
