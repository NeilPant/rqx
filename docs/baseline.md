# Baseline

A ten-year-old service has four thousand findings. Turn the gate on and it fails on
the first pull request, so it gets turned off on the second. A baseline is how you
gate on a codebase that already has debt: record what is there today, and let the
gate judge only what a change adds.

```sh
rqx baseline
```

That scans the project and writes `.rqx/baseline.json` — one entry per finding,
sorted, keyed by an identity that survives the finding moving down the file. Commit
it. Then point the gate at it:

```yaml
gate:
  fail_on: warning
  baseline: .rqx/baseline.json
```

Now `rqx check` and a gated `rqx scan` count only findings the baseline does not
already hold:

```
  Findings  0 errors · 8 warnings · 2 info
  Baseline  7 findings known · 1 new
```

The eight warnings are still reported, still shown in the editor, still in the JSON.
Seven of them are debt the baseline recorded; one arrived with this change, and that
one is what fails the build.

## What a baseline does not change

`min_health` and `min_grade` still see every finding. The health score is a
statement about the code, not about which debt has been agreed to — a project that
baselines four thousand findings still scores what four thousand findings deserve.
Only `fail_on` and `max_findings` respect the baseline.

## Keeping it honest

A baseline that only ever grows is a permanent exemption. Two things stop that.

`rqx baseline --update` refuses to record more findings than the last one held:

```
Error: this would record 412 findings against the 408 already recorded.
Fix them, or pass --force to accept the larger set.
```

And the ratchet fails the gate when the total grows at all, whether or not the new
findings are in files anybody reviewed:

```yaml
gate:
  baseline: .rqx/baseline.json
  ratchet: true
```

```
  10 findings against the 6 the baseline recorded; remove 4 or run `rqx baseline --update`
```

Run `rqx baseline --update` on the default branch after a cleanup and commit the
smaller file. The number only goes down.

## Identity

An entry is keyed on the rule, the file, and what the finding is about — the name of
the function, the binding, the import. Never the line, and never the measured value.
So a finding survives the code above it growing, and survives its own complexity
dropping from 25 to 22. It does not survive the file being renamed or the function
being renamed, which both read as one finding resolved and one introduced.

The identity is FNV-1a, written out rather than taken from the standard library's
hasher, because the number is committed to your repository and has to mean the same
thing on the next toolchain.

## Interaction with waivers

A waiver and a baseline answer different questions. A waiver says *this finding is
wrong or acceptable, and here is why* — see [suppressions](suppressions.md). A
baseline says *this finding is real and we have not got to it yet*. Waived findings
never reach the count at all; baselined ones are counted, reported, and excluded
only from the gate.
