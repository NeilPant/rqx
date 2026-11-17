# Where the score comes from

The health score is a single number. `rqx health` shows the same calculation divided
into buckets.

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

`found` is the number of findings in the bucket. `debt` is the bucket's weight in the
score calculation (severity times reach), as a share of the total. The two columns can
rank buckets differently. In the output above, the 24 `size` findings carry more
weight than the 9 `coupling` findings. In another project, sixteen coupling findings
carried a third of the weight of seventeen flow findings.

## Two things this tells you

Where the score went. Buckets are sorted by debt, so the top row is the one to fix
first if only one is addressed.

When the score has stopped being informative. At high finding density the score
reaches its floor, and removing any single bucket still leaves it at zero. In that case
rqx reports it and omits the `without` column, instead of printing `0 (+0)` on every
row:

```
    the score stays at 0 whichever single bucket goes, so `debt` is what separates
    them: the weight each carries in the same arithmetic
```

In that state the score does not distinguish between buckets. Set a
[baseline](baseline.md) and gate on what a change adds.

## When there is room to move

When the score is not at its floor, a third column appears:

```
               found    debt   without  worst of it
    waste           12     41%   78 (+9)  unused-export, unused-file
    size             4     22%   74 (+5)  long-function
```

`without` is the score recomputed with that bucket removed, not an apportioned share.
The score curve is exponential, so the gains are not additive: removing waste and size
together improves the score by less than 9 + 5.

## Grouping

```sh
rqx health --by rule    # which rule costs most
rqx health --by dir     # which part of the tree
rqx health --by file    # which file
```

`--by rule` shows whether a costly family comes from one rule firing in many places or
from several rules.

`--json` prints one object per bucket, with `debtShare`, `healthWithout` and `gain`.
