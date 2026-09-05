# What to do about it

`rqx scan` says what is wrong. `rqx plan` orders the findings. Neither answers the
questions a lead asks before there is a plan.

```sh
rqx recommend
```

## Configuration

Is the configuration describing this codebase or fighting it?

```
    raise `thresholds.cognitive_complexity` from 15 to about 22
      41% of functions are at or above it, so it is describing normal code here
      rather than the exceptions — p90 is 22, p99 is 40
```

A threshold most of the codebase sits above is not a standard; it is a rule that
fires on normal code, and every finding it produces is noise that trains people to
ignore the tool. One nothing reaches is the same failure the other way round — it
cannot fire at all.

The common case is neither, and then this section stays quiet. A long tail under the
threshold with a handful of real offenders over it is the rule working.

It also names rules firing more than twenty times per thousand lines:

```
    decide about `distant-declaration`
      9228 findings, 20 per thousand lines — at that rate it is either the
      project's biggest problem or the wrong rule for it
```

That is not a recommendation to turn it off. It is a recommendation to decide, once,
on purpose, rather than living with a number nobody chose.

## Refactoring

Which single technique would answer the most at once, and where.

```
    Extract Variable answers 6367 findings
      184 of them are under `finql-app/src/app/(private)`
```

One technique answering hundreds of findings in one directory is a week's work with a
shape to it. The same findings scattered across sixty techniques is just a list, and
`rqx plan` is the better tool for that.

## Reading order

Where a new person should start.

```
    finql-api/src/container.ts
      105 importers · 12 commits · 148 findings · grade critical
```

Ranked by reach times churn times trouble. A file nobody imports and nobody changes
is not where anyone should start however bad it is; a file a hundred modules import
is worth understanding even if it is clean.

## Narrowing and scripting

```sh
rqx recommend --only config
rqx recommend --only refactor
rqx recommend --only reading
rqx recommend --json
```

Every recommendation carries the evidence that produced it, in the line beneath. A
recommendation you cannot check is one you have to trust, and nothing else in this
tool asks for that.
