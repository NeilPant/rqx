# Baseline

A baseline records the findings a project has today so that the gate evaluates only
the findings a change adds. It allows a project with existing findings to enable the
gate without failing on all of them.

```sh
rqx baseline
```

The command scans the project and writes `.rqx/baseline.json`. The file has one entry
per identity, sorted. An identity does not change when the finding moves to a
different line, and each entry records how many findings have it (see
[Repeated findings](#repeated-findings)). Commit the file, then point the gate at it:

```yaml
gate:
  fail_on: warning
  baseline: .rqx/baseline.json
```

`rqx check` and a gated `rqx scan` then count only findings that the baseline does not
already hold:

```
  Findings  0 errors · 8 warnings · 2 info
  Baseline  7 findings known · 1 new
```

All eight warnings are still reported, shown in the editor and included in the JSON
output. Seven are recorded in the baseline. The eighth is new, and it is the only one
that counts against the gate.

## What a baseline does not change

`min_health` and `min_grade` evaluate every finding, including baselined ones. The
health score describes the code as it is, so recording findings in a baseline does not
raise it. Only `fail_on` and `max_findings` respect the baseline.

## Keeping it honest

A baseline that is only ever regenerated with more findings works as a permanent
exemption. Two mechanisms limit this.

`rqx baseline --update` refuses to record more findings of any rule than the previous
baseline held for that rule, even when the total falls:

```
Error: this would record more findings than the baseline holds for these rules:
deep-nesting from 7 to 9, magic-number from 5 to 6. Fix them, or pass --force to
accept the larger set.
```

A baseline file written by rqx 0.5.0 or earlier has no counts, so each rule is compared
by its number of entries, and the total number of findings is compared as well.

The ratchet fails the gate whenever the total number of findings exceeds the number
the baseline recorded. It applies regardless of which files the new findings are in:

```yaml
gate:
  baseline: .rqx/baseline.json
  ratchet: true
```

```
  10 findings against the 6 the baseline recorded; remove 4 or run `rqx baseline --update`
```

After a cleanup, run `rqx baseline --update` on the default branch and commit the
smaller file. Without `--force`, the recorded count of a rule cannot increase.

## Identity

An entry is keyed on the rule, the file, and the subject of the finding, such as the
name of the function, the binding or the import. The line number and the measured
value are not part of the key. A finding therefore keeps its identity when code above
it changes length, and when its measured value changes (for example, complexity
dropping from 25 to 22). Renaming the file or the function changes the identity, so
the finding counts as one resolved finding and one new finding.

The identity is an FNV-1a hash. rqx implements it directly instead of using the
standard library's hasher, because the value is stored in a committed file and must be
the same across toolchain versions.

## Repeated findings

Several findings can have the same identity. For example, two `discarded-error`
findings for calls to the same function in one file have the same rule, file and
subject. The entry for that identity records how many findings had it in a `count`
field, which is written only when the count is greater than 1.

A scan matches findings to entries by count. When a file has more findings with an
identity than the entry records, the extra findings are new. Findings are matched in
source order, so the findings reported as new are the last ones in the file, which
are not always the lines a change added.

`gate.baseline_match` selects how findings are matched:

| Value | Effect |
|---|---|
| `count` (default) | An entry covers as many findings with its identity as it records. Any more are new. |
| `set` | An entry covers every finding with its identity, however many there are. rqx 0.5.0 and earlier matched this way. |

```yaml
gate:
  baseline: .rqx/baseline.json
  baseline_match: count
```

A baseline file written by rqx 0.5.0 or earlier has no counts, and each of its
entries is read as one finding. Under `count`, repeated findings that such a file did
not distinguish are reported as new until `rqx baseline --update` records the counts.
With `set`, the result is the same as in 0.5.0.

## Interaction with waivers

A waiver and a baseline entry have different meanings. A waiver records that a finding
is incorrect or acceptable, together with the reason (see
[suppressions](suppressions.md)). A baseline entry records that a finding is valid and
has not been fixed yet. Waived findings are not counted at all. Baselined findings are
counted and reported, and are excluded only from the gate.
