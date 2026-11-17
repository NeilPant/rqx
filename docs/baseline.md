# Baseline

A baseline records the findings a project has today so that the gate evaluates only
the findings a change adds. It allows a project with existing findings to enable the
gate without failing on all of them.

```sh
rqx baseline
```

The command scans the project and writes `.rqx/baseline.json`. The file has one entry
per finding, sorted, and each entry is keyed by an identity that does not change when
the finding moves to a different line. Commit the file, then point the gate at it:

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

`rqx baseline --update` refuses to record more findings than the previous baseline
held:

```
Error: this would record 412 findings against the 408 already recorded.
Fix them, or pass --force to accept the larger set.
```

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
smaller file. Without `--force`, the recorded total cannot increase.

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

## Interaction with waivers

A waiver and a baseline entry have different meanings. A waiver records that a finding
is incorrect or acceptable, together with the reason (see
[suppressions](suppressions.md)). A baseline entry records that a finding is valid and
has not been fixed yet. Waived findings are not counted at all. Baselined findings are
counted and reported, and are excluded only from the gate.
