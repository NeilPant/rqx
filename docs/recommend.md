# What to do about it

`rqx scan` reports findings, and `rqx plan` orders them. `rqx recommend` answers
three questions that come before a plan: whether the configuration fits the
codebase, which refactoring addresses the most findings, and which files to read
first.

```sh
rqx recommend
```

## Configuration

Whether each threshold fits the codebase.

```
    raise `thresholds.cognitive_complexity` from 15 to about 22
      41% of functions are at or above it, so it is describing normal code here
      rather than the exceptions — p90 is 22, p99 is 40
```

A threshold that most of the codebase exceeds fires on typical code, so most of its
findings do not distinguish problem code from normal code. A threshold that nothing
reaches never fires.

When neither applies, this section prints nothing. A distribution with a long tail
under the threshold and a few functions over it is the intended case.

It also names rules firing more than twenty times per thousand lines:

```
    decide about `distant-declaration`
      9228 findings, 20 per thousand lines — at that rate it is either the
      project's biggest problem or the wrong rule for it
```

The recommendation is to decide whether to keep the rule. It does not recommend
disabling it.

## Refactoring

The single technique that addresses the most findings, and where.

```
    Extract Variable answers 6367 findings
      184 of them are under `finql-app/src/app/(private)`
```

When one technique addresses many findings in one directory, the work can be done as
a single refactoring. When the same number of findings is spread across many
techniques, `rqx plan` is the better tool.

## Reading order

Where a new reader should start.

```
    finql-api/src/container.ts
      105 importers · 12 commits · 148 findings · grade critical
```

Files are ranked by reach (importers) times churn (commits) times trouble
(findings). A file that nothing
imports and nobody changes ranks low however many findings it has. A file that
many modules import ranks high even if it is clean.

## Narrowing and scripting

```sh
rqx recommend --only config
rqx recommend --only refactor
rqx recommend --only reading
rqx recommend --json
```

Every recommendation carries the evidence that produced it, in the line beneath.
