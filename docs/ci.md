# rqx in CI

rqx runs one analysis pass per invocation and reports the result through an exit code
and one or more reporters. Every integration below uses the same command with a
different `--reporter`.

## Exit codes

| Exit code | Meaning |
|---|---|
| `0` | The run completed and the gate passed. |
| `1` | The run completed and the gate failed. |
| `2` | rqx could not run: bad path, invalid configuration or unresolvable revision. |
| `101` | rqx hit an internal error. Report it as a bug. |

A closed output pipe is not a failure. `rqx scan . | head` exits `0`.

## Gating

`rqx scan` gates only when a gate flag is passed. `rqx check` always gates, using the
`gate` mapping from `rqx.yaml` unless a flag overrides it.

| Flag | Fails when |
|---|---|
| `--fail-on <severity>` | Any finding is at or above that severity. |
| `--allow-findings <n>` | More than `n` such findings exist. |
| `--min-health <n>` | The project score is below `n`. |
| `--min-grade <grade>` | Any single file grades below that floor. |

`--min-grade` evaluates each file separately. A project-wide score averages a
low-graded file with the rest, and severity applies per finding. Grades, from best to
worst, are `pristine`, `clean`, `fair`, `poor` and `critical`. The value `never` turns
the grade floor off.

Passing any gate flag makes the scan gate, and the other `gate` keys from `rqx.yaml`
then apply as well. `--min-grade poor` on a project with `gate.fail_on: error` gates
on both.

`fail_on: never` disables only the severity count. The health floor, the grade floor,
the ratchet and `fail_on_incomplete` still apply when they are set.

Each flag has an environment variable:

| Variable | Same as |
|---|---|
| `RQX_FAIL_ON` | `--fail-on` |
| `RQX_ALLOW_FINDINGS` | `--allow-findings` |
| `RQX_MIN_HEALTH` | `--min-health` |
| `RQX_MIN_GRADE` | `--min-grade` |
| `RQX_FAIL_ON_INCOMPLETE` | `--fail-on-incomplete`. `1`, `true` or `yes` turns it on. `0`, `false` or an empty value leaves it off. |

Precedence is flag, then environment variable, then `rqx.yaml`. Setting any of these
variables makes `rqx scan` gate, as a flag does, so a variable set in a shared runner
image changes the exit code of every scan on that runner. A variable set to an empty
string counts as unset. A workflow passes an empty string for an input that has no
value.

A key that a [policy](policy.md) locks is not overridden. A flag or variable that gives it
a different value stops the run with exit code 2.

## Reporters

`--reporter <name>` writes to stdout. `--reporter <name>=<path>` writes to a file.
Both forms are repeatable, so one run can produce every output a pipeline needs:

```sh
rqx scan . \
  --reporter terminal \
  --reporter junit=rqx.xml \
  --reporter sarif=rqx.sarif \
  --fail-on error
```

| Reporter | Consumer |
|---|---|
| `terminal` | The job log. |
| `json` | Newline-delimited JSON: one object per finding, then a summary. See [json.md](json.md) for the stability guarantees. |
| `sarif` | GitHub code scanning, Azure DevOps advanced security. |
| `junit` | GitLab, Jenkins, CircleCI, Azure Pipelines, Bitbucket, Buildkite. |
| `gitlab` | GitLab Code Quality, which shows findings as merge request annotations. |
| `github` | GitHub Actions annotations on the diff. |
| `markdown` | Job summaries, merge request comments, chat. |
| `html` | A report opened from a pipeline's artifacts. See [HTML report](#html-report). |

`terminal` and `github` can share stdout. Every other reporter writes a document and
needs stdout, or a file, to itself.

The `json`, `sarif`, `junit`, `gitlab` and `github` reports list every finding. The
`terminal`, `markdown` and `html` reports list the first 500 in report order
(`scan.max_findings`) and say how many they left out. `--max-findings N` caps every
report at `N`, and `--max-findings 0` lists every finding everywhere. When a cap leaves
findings out of a SARIF report, the run carries the count as
`runs[0].properties.rqx.omitted` and a `warning` in
`runs[0].invocations[0].toolExecutionNotifications`; the JSON summary carries it as
`omitted`. The gate and the health score count every finding whatever the cap.

The commands that print their own view instead of the findings (`fix`, `verify`,
`plan`, `suppressions`, `complexity`, `perf`, `recommend`, `slop`, `hotspots`, `health`,
`patterns`, `graph` and `config`) also take `--reporter`, with two names: `terminal`
and `json`. `rqx health --reporter terminal --reporter json=health.json` prints the
table and writes the document. On these commands `--json` is short for `--reporter
json`. `graph` additionally accepts `text`, `dot` and `mermaid`, which are its
`--format` values.

Colour follows `NO_COLOR` and whether stdout is a terminal. `--color never` turns it
off. `--color always` turns it on, for log viewers that render ANSI.

On a terminal that supports hyperlinks (iTerm2, WezTerm, VS Code, Ghostty, kitty,
Windows Terminal, VTE 0.50 and later) the report makes each `file:line` clickable.
`--hyperlinks auto|always|never`, or `RQX_HYPERLINKS`, overrides the detection.
`always` is for a log viewer that supports OSC 8 links, and it still requires colour.
A link opens the file by default. `RQX_EDITOR_URL` substitutes an editor's own URL
scheme, with `{path}`, `{line}` and `{column}` filled in, such as
`vscode://file/{path}:{line}:{column}`. A report written to a file or a pipe contains
no links unless colour is forced on.

`rqx scan` and `rqx check` send a long `terminal` report through a pager, as git does
with its output. The pager is `$RQX_PAGER`, else `$PAGER`, else `less -FRX`. The report
is paged only when all of these hold:

- stdout is a terminal and no CI variable is set;
- every reporter that writes to stdout is `terminal`;
- the report needs more rows than the window has, keeping one row for the shell
  prompt;
- the run is not `rqx scan --watch`.

Everything else, including a pipe, a file and a document reporter, is printed
without a pager. `--no-pager`, an empty `RQX_PAGER` or a pager of `cat` turns paging off. An
empty `PAGER` counts as unset. If the pager cannot be started, the report is printed
directly. Quitting the pager before the end of the report leaves the exit code
unchanged, and while the pager runs, `ctrl-c` goes to the pager and does not stop
rqx. The pager command is split on whitespace and quotes are not interpreted. When
`LESS` is not set, rqx sets it to `FRX` for the pager, so `PAGER=less` keeps the
colours. Output on stderr, such as progress and the `-vv` timings, is never paged.

The progress bar is drawn only when stderr is a terminal and the run is not in CI.
`--progress` (or `RQX_PROGRESS`) selects what a scan reports:

| `--progress` | Behaviour |
| --- | --- |
| `auto` | The bar on a terminal outside CI, nothing otherwise. Default. |
| `bar` | The bar whenever stderr is a terminal, CI included. |
| `plain` | One line on stderr every two seconds and at each phase, with or without a terminal. |
| `none` | Nothing. |

`plain` suits a long job whose log would otherwise stay silent for minutes:

```
rqx: scanning 1,204/3,456 files · 31 findings · 4.2s
rqx: grouping duplicates · 3,456 files · 31 findings · 9.0s
```

`--no-progress`, or `RQX_NO_PROGRESS=1`, is the same as `--progress none`, and `--quiet`
implies it. Progress goes to stderr only, so stdout and report files are identical for
every setting.

### Where the verdict is reported

A gated run writes its verdict into each report as well as into the exit code:

| Reporter | Location |
|---|---|
| `terminal` | The `gate passed` or `gate failed` line, with each failure listed under it. |
| `json` | `gate` on the summary line: `{ "passed": bool, "failures": [...] }`. |
| `junit` | A `<testsuite name="rqx gate">` with one test, which fails when the gate fails. The totals on `<testsuites>` include it. |
| `github` | The last annotation: `::error title=rqx gate::` with the failures, or `::notice title=rqx gate::gate passed`. |
| `sarif` | `runs[0].invocations[0].exitCode`, one `toolExecutionNotifications` entry per failure at level `error`, and `runs[0].properties.rqx.gate`. |

A scan that was not gated has no verdict, so these fields are absent instead of
reading `passed`. `rqx check` and `rqx comment` always have one.

### SARIF and GitHub code scanning

Code scanning matches the alerts in a new upload against the alerts it already holds.
Two variables change that matching, and both are off by default:

| Variable | Effect |
|---|---|
| `RQX_SARIF_FINGERPRINTS=1` | Adds `partialFingerprints` to every result. The fingerprint is built from the finding's identity, the same one the GitLab report uses, so an alert stays attached to its finding when code above it moves. |
| `RQX_SARIF_CATEGORY=<id>` | Adds `automationDetails.id`, which files the run under that category. |

A review that reports [complexity regressions](#complexity-regressions) adds results of
the rule `complexity-regression` to the same run.

Enabling either variable on a repository that already has open alerts re-keys them.
GitHub sees new fingerprints or a new category, closes the old alerts and opens the
same findings again. Set a category when one repository uploads several SARIF runs
that would otherwise overwrite each other.

### HTML report

> Deprecated: the `html` reporter will be removed in 0.6.0. Use `json` or `sarif` for
> a report kept as an artifact.

`html` writes one self-contained file: findings grouped by family and rule, then every
file, worst grade first, with its code frames, then the gate's verdict when there is
one.
The file embeds its styles, loads nothing from the network and works without
JavaScript. It follows the system's light or dark setting.

GitHub Actions:

```yaml
- run: rqx scan . --reporter terminal --reporter html=rqx.html --fail-on error
- uses: actions/upload-artifact@v4
  if: always()
  with:
    name: rqx-report
    path: rqx.html
```

GitLab CI, alongside the other reports:

```yaml
  script:
    - rqx scan . --reporter terminal --reporter html=rqx.html --fail-on error
  artifacts:
    when: always
    paths: [rqx.html]
```

## GitHub Actions

```yaml
- uses: actions/checkout@v4
  with:
    fetch-depth: 0          # `check` diffs against a base
- uses: jadhamwi21/rqx@v0.5.3
  with:
    base: ${{ github.base_ref }}
    fail-on: error
```

The job needs `security-events: write` for the SARIF upload to code scanning:

```yaml
permissions:
  contents: read
  security-events: write
```

A repository whose default token is read-only, a private repository without code
scanning, and a pull request from a fork all lack that permission. Set
`upload-sarif: "false"` for them; the scan, the annotations and the summary still run.

| Input | Default | Does |
|---|---|---|
| `path` | `.` | Directory to analyse |
| `base` | empty | Revision to compare against; empty scans the whole project |
| `config` | empty | Path to a configuration file such as `rqx.yaml` |
| `fail-on` | empty | Lowest severity that fails the job: `error`, `warning`, `info` or `never`; empty uses `rqx.yaml` |
| `sarif-file` | `rqx.sarif` | Where the SARIF report is written |
| `upload-sarif` | `"true"` | Upload the SARIF report to GitHub code scanning |
| `cache` | `"true"` | Carry `.rqx/cache.bin` from one run to the next |

| Output | Holds |
|---|---|
| `findings` | The number of findings reported |
| `passed` | Whether the gate passed |

The tag in `uses:` and the `rev:` of the pre-commit hook below name a release. Pin the
one you want; `tools/release.sh` keeps the examples in this repository on the current
release.

The action runs the analysis once and produces the log, inline annotations, the SARIF
upload and a job summary from that pass. It installs rqx by downloading the binary
released with its own tag and verifying it against `SHA256SUMS`. A ref with no
published binary, such as a fork or `@main` ahead of the last release, compiles rqx
from the checkout instead.

### A health badge

`rqx badge --output badge.svg` writes the health score as a small self-contained SVG:
`rqx` on the left, and the score with its grade, such as `health 82 fair`, on the
right. The right side takes the colour of the grade: green for `pristine` and `clean`,
tan for `fair`, amber for `poor` and red for `critical`. The file has no external
references, so a README, a wiki or a static site can serve it directly. Without
`--output`, the SVG is written to standard output.

This workflow regenerates the badge on every push to the default branch and keeps it
on a `badges` branch, which a README embeds from `raw.githubusercontent.com`:

```yaml
name: rqx badge
on:
  push:
    branches: [main]
permissions:
  contents: write
jobs:
  badge:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - name: Install rqx
        run: |
          curl -fsSL https://raw.githubusercontent.com/jadhamwi21/rqx/main/install.sh | sh
          echo "$HOME/.local/bin" >> "$GITHUB_PATH"
      - name: Write the badge
        run: rqx badge --output "$RUNNER_TEMP/rqx-health.svg"
      - name: Publish it to the badges branch
        run: |
          git config user.name "github-actions[bot]"
          git config user.email "41898282+github-actions[bot]@users.noreply.github.com"
          git fetch origin badges || git switch --orphan badges
          git switch badges
          cp "$RUNNER_TEMP/rqx-health.svg" rqx-health.svg
          git add rqx-health.svg
          git diff --cached --quiet || git commit -m "Update the rqx health badge"
          git push origin badges
```

The README then embeds it:

```markdown
![rqx health](https://raw.githubusercontent.com/OWNER/REPO/badges/rqx-health.svg)
```

### The scan cache

rqx stores one analysis per file in `.rqx/cache.bin` and skips a file whose entry is
still valid. The action restores this file between runs with `actions/cache`, so a
pull request re-analyses only the files it changed. The cache is on by default.
`cache: "false"` turns it off.

A fresh clone can still use the cache. A lookup matches on path and modification time
first. A clone has new modification times, so on a mismatch rqx reads the file and
compares a hash of its contents. The cache key combines the rqx version, a hash of the
`rqx.yaml` files, the OS and the commit. The commit makes every run save its own
cache, and a run restores the newest cache with the same version, configuration and
OS. A cache written under another rqx version or other settings is ignored whatever
the key says, so a stale restore costs a full scan.

Without the action, cache `.rqx/cache.bin` directly. GitLab form:

```yaml
rqx:
  cache:
    key:
      prefix: rqx
      files: [rqx.yaml]       # a new cache whenever the config changes
    paths: [.rqx/cache.bin]
    when: always              # saved when the gate fails too
```

When the job installs a specific rqx version, include it in the prefix
(`prefix: rqx-0.5.3`). The cache only affects speed. A missing, outdated or damaged
file is rebuilt on the next scan, and no report depends on it.

## GitLab CI

```yaml
rqx:
  image: rust:latest
  script:
    - curl -fsSL https://raw.githubusercontent.com/jadhamwi21/rqx/main/install.sh | sh
    - export PATH="$HOME/.local/bin:$PATH"
    - rqx check . --base "origin/$CI_MERGE_REQUEST_TARGET_BRANCH_NAME"
        --reporter terminal
        --reporter gitlab=gl-code-quality-report.json
        --reporter junit=rqx-junit.xml
  artifacts:
    when: always
    reports:
      codequality: gl-code-quality-report.json
      junit: rqx-junit.xml
```

Fingerprints in the Code Quality report exclude the line number, so a change at the
top of a file does not report every finding below it as new. When a file holds the
same finding more than once, the repeats are distinguished by ordinal and not by
position. GitLab shows one annotation per fingerprint, so without the ordinal only the
first of several identical findings in a file would reach the merge request.

## Running a job locally

`gitlab-runner exec` was removed in GitLab 16, and `run-single` polls a real GitLab
instance for work, so neither runs a pipeline from a checkout. `tools/ci-local.sh`
reads a job's `script:` from `.gitlab-ci.yml` and runs it locally:

```bash
tools/ci-local.sh                 # the jobs in the file
tools/ci-local.sh lint            # run one
tools/ci-local.sh rqx --in ../app # run it against another checkout
```

The script is not a runner. It applies no image, cache, artifacts or `rules:`, so a job
that passes locally can still fail in CI because of the container. It checks the
commands, their order and the exit code.

## CircleCI

```yaml
- run: |
    curl -fsSL https://raw.githubusercontent.com/jadhamwi21/rqx/main/install.sh | sh
    echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$BASH_ENV"
- run:
    command: rqx scan . --reporter terminal --reporter junit=/tmp/rqx/results.xml --fail-on error
    when: always
- store_test_results:
    path: /tmp/rqx
```

## Jenkins

```groovy
sh 'rqx scan . --reporter terminal --reporter junit=rqx-junit.xml --fail-on error'
junit testResults: 'rqx-junit.xml', allowEmptyResults: true
```

To mark the build unstable instead of failed when the gate fails, wrap the `sh` step
in `catchError(buildResult: 'UNSTABLE', stageResult: 'FAILURE')`.

## Azure Pipelines

```yaml
- script: rqx scan . --reporter terminal --reporter junit=$(Build.ArtifactStagingDirectory)/rqx.xml --fail-on error
  displayName: rqx
- task: PublishTestResults@2
  condition: always()
  inputs:
    testResultsFormat: JUnit
    testResultsFiles: $(Build.ArtifactStagingDirectory)/rqx.xml
```

## Bitbucket Pipelines

```yaml
- step:
    script:
      - rqx scan . --reporter terminal --reporter junit=test-reports/rqx.xml --fail-on error
```

Bitbucket collects results under `test-reports/` automatically.

## Other providers

A provider that reads JUnit XML can use the `junit` reporter:

```sh
rqx scan . --reporter terminal --reporter junit=rqx.xml --fail-on error
```

Otherwise, parse the NDJSON output:

```sh
rqx scan . --reporter json \
  | jq -c 'select(.kind != "summary") | {file: .location.file, rule, severity}'
```

## Validating the configuration

A scan with an invalid `rqx.yaml` stops at the first error. `rqx config validate`
reports every problem in the configuration before any code is read:

```sh
rqx config validate --strict
```

An error exits `2`, the same code as a run that could not start. A warning, such as a
rule name that is not in the catalogue, exits `0` by default and `1` under `--strict`.
With `--json`, the command prints one line per problem with its file, line and column.
See [json.md](json.md).

## Functions that repeat existing code

When a change adds a function, `rqx check` and `rqx comment` also scan the revision the
change forked from, the merge base of `--base` and `HEAD`. A function that exists only in the working tree and whose
body matches a function that exists at both revisions is reported as
`reimplemented-existing`. The finding names the existing function and its location.

| Match | Confidence |
|---|---|
| The same normalised syntax, differing only in local names. | `verified` |
| The same control flow and constants in one language, with other member names, in a body of at least 8 statements and 40 syntax nodes. | `heuristic` |
| The same control flow and constants in a file of another language, with the same size floor. | `low` |

A function that was renamed or moved is not reported, because the function it replaced
no longer exists to call. `clone_min_statements` and `clone_min_nodes` set the smallest
body that is compared. `--files-from` names no revision, so the rule does not run
there, and `rqx scan` never reports it.

The base scan costs time, so it is limited in two ways. It runs only when a new or
changed file holds a named function that the file did not hold at the base. Both sides
of each such file are analysed on their own to decide this, without a checkout. A change
that adds no function is reviewed on one scan and reports neither this rule nor the
`Resolved` credit described under [Commenting on the merge request](#commenting-on-the-merge-request).
When the base is scanned, it reads the `.rqx/cache.bin` of the working tree and never
writes it, so a file with the same content at both revisions is not analysed again.

## Keeping a health trend

`rqx scan --record` appends a snapshot to `.rqx/trend.ndjson`, and `--trend` draws that
file in the HTML report. A CI runner starts from a clean checkout, so the file has to be
restored before the scan and saved after it. See [trends.md](trends.md) for the format.

GitHub Actions record on the default branch and chart on every run. Cache keys cannot
be overwritten, so each run saves a new key and the next run restores the newest one
by prefix:

```yaml
- uses: actions/cache/restore@v4
  with:
    path: .rqx/trend.ndjson
    key: rqx-trend-${{ github.run_id }}
    restore-keys: rqx-trend-
- run: >
    rqx scan .
    ${{ github.ref == 'refs/heads/main' && '--record' || '' }}
    --trend .rqx/trend.ndjson
    --reporter terminal --reporter html=rqx.html
- uses: actions/cache/save@v4
  if: github.ref == 'refs/heads/main'
  with:
    path: .rqx/trend.ndjson
    key: rqx-trend-${{ github.run_id }}
- uses: actions/upload-artifact@v4
  with:
    name: rqx-report
    path: rqx.html
```

Branches and pull requests read the history without recording to it, so the chart shows
the default branch's trend.

GitLab CI keeps the file in the job cache. `policy: pull-push` restores it before the
script and saves it after, and `when: always` saves it when the gate fails:

```yaml
rqx-trend:
  rules:
    - if: $CI_COMMIT_BRANCH == $CI_DEFAULT_BRANCH
  cache:
    key: rqx-trend
    paths: [.rqx/trend.ndjson]
    policy: pull-push
    when: always
  script:
    - rqx scan . --record --trend .rqx/trend.ndjson --reporter html=rqx.html
  artifacts:
    paths: [rqx.html]
```

A CI cache is evicted when it is not used for a while, and the history it held is lost
with it. A pipeline that needs a durable history can upload the file as an artifact,
copy it to a branch or a bucket after each run, or commit it. A trend file that is
missing at the start of a run is not an error for `--trend`: the report notes that a
chart needs two snapshots, and `--record` starts a new file.

## Review without git history

`rqx check` normally asks git which files differ from `--base`. A shallow clone has no
common commit to diff from, and some pipelines already know which files changed.
`--files-from` accepts the list instead:

```sh
git diff -z --name-only origin/main... | rqx check --files-from -
```

`-z` separates names with NUL, which `--files-from` detects, so a file name that
contains a space or a newline is read correctly. Without `-z`, one path per line also
works. The list replaces the diff as the set of files under review. Files that import
a listed file are reviewed as well, and the whole project is still analysed, so a
finding that depends on another file stays correct. A path that is not a file under
the scanned directory is reported once on stderr and skipped.

Any program can produce the list, for example a changed-files API call or the
affected-projects output of a build tool. `--base` and `--files-from` cannot be
combined.

## Review what is staged

`rqx check --staged` reviews what `git commit` would record:

```sh
git add src/router.ts
rqx check --staged
```

The staged files are read from the index, not the working tree. rqx builds a temporary
copy of the repository in which every staged file has its staged content, scans that
copy, and removes it afterwards. An unstaged edit to a staged file cannot change the
verdict in either direction: unstaged bad code passes, and an unstaged fix does not
rescue staged bad code. The working tree is never written to.

Every file that is not staged is read from the working tree, so a staged file is judged
against its neighbours as they are on disk. Whatever imports a staged file is reviewed
as well, and the gate is the same one `rqx check` uses. When nothing is staged, nothing
is reviewed and the check passes.

`--staged` cannot be combined with `--base`, `--files-from`, `--fix` or `--write`. It
needs a git repository, and a path given to it must be a directory. In a repository
subdirectory, `rqx check packages/app --staged` reviews the staged files under that
directory.

## Checking before every commit

`rqx check --staged` exits 1 when the gate fails, which is what a git hook needs. Each
snippet below runs it before a commit is recorded. Pass the usual gate flags after it,
such as `--fail-on error`. Committing with `--no-verify` skips the hook. A hook run for
a commit made with `-a` or with a path reads the index git prepared for that commit, so
the review matches what is recorded.

### pre-commit

The repository ships a [pre-commit](https://pre-commit.com) hook definition in
`.pre-commit-hooks.yaml`. It runs `rqx check --staged` as a `system` hook, so `rqx`
must be on the `PATH`. It passes no file names, because rqx reads the index itself, and
`always_run: true` keeps it running on commits that pre-commit would otherwise skip.
It needs pre-commit 3.2.0 or later.

```yaml
repos:
  - repo: https://github.com/jadhamwi21/rqx
    rev: v0.5.3
    hooks:
      - id: rqx-check
```

Use a release tag that contains `.pre-commit-hooks.yaml`. To add gate flags, override
the entry:

```yaml
      - id: rqx-check
        entry: rqx check --staged --fail-on error
```

### A plain git hook

Save this as `.git/hooks/pre-commit` and make it executable with
`chmod +x .git/hooks/pre-commit`:

```sh
#!/bin/sh
exec rqx check --staged
```

`.git/hooks` is not versioned. To share the hook, keep the script in the repository, for
example in `.githooks/pre-commit`, and set the `core.hooksPath` configuration value to
that directory.

### lefthook

In `lefthook.yml`:

```yaml
pre-commit:
  commands:
    rqx:
      run: rqx check --staged
```

### husky

In `.husky/pre-commit`:

```sh
rqx check --staged
```

## Other tools' results

`--sarif-in FILE` reads a SARIF 2.1.0 file that another linter wrote and reports each
result as a finding in the `external` family. It is accepted by `scan`, `check` and
`comment`, can be repeated, and can be set as `scan.sarif_in` in `rqx.yaml`.

```sh
ruff check --output-format sarif --output-file ruff.sarif
rqx check --base origin/main --sarif-in ruff.sarif --fail-on error
```

| Property | Value |
|---|---|
| Rule id | The tool name in lower case, a slash, and the tool's rule id: `eslint/no-unused-vars`, `ruff/F401`. |
| Severity | SARIF `error` maps to `error`, `warning` and an absent level to `warning`, `note` and `none` to `info`. A result without a level uses the rule's `defaultConfiguration.level`. |
| Location and message | The first physical location and the message text, with line breaks collapsed. The rule's help text becomes the fix hint. |
| Skipped | Results suppressed in the SARIF file, results without a physical location, and results in files outside the scanned directory. |
| Unknown fields | Ignored. |

External findings take part in baselines, `gate.fail_on`, every
reporter and the pull request comment. They are not part of the health score: the
severities of other tools are not calibrated against the weights of the rqx catalogue,
so counting them would make the score depend on which linters a project runs. A file
that cannot be read or is not SARIF is reported on stderr and skipped. `rqx baseline`
has no `--sarif-in` flag; it records external findings when `scan.sarif_in` is set in
`rqx.yaml`.

## Adopting a gate on an existing codebase

A gate that fails on the first run is often disabled. To avoid that, start from the
current state and tighten the gate gradually:

```yaml
# rqx.yaml
gate:
  fail_on: error        # error-severity findings only
  max_findings: 0
  min_health: 0
```

Then use `rqx check --base main`, which reviews the changed files and the files that
import them, so the gate applies to new work and not to existing findings.

## Failing on a half-applied pattern

`gate.fail_on_incomplete: true` in `rqx.yaml`, or `--fail-on-incomplete`, is the only
way design pattern advice affects the exit code. It considers half-applied patterns
only: a decorator that cannot substitute for the object it wraps, or a subscribe with
no matching unsubscribe. A pattern that is in use or that would fit does not affect
the exit code, and pattern advice does not change the health score in any
configuration. `rqx check` and `rqx comment` apply the option to the files they
review. `rqx scan` applies it to all files.

## Complexity regressions

`rqx check` and `rqx comment` can list the functions whose time order grew against the
base revision, such as `reconcileBatch` going from `O(n)` to `O(n²)`. The orders are the
ones `rqx perf` reports, composed across calls and files (see [perf.md](perf.md)). A
function whose own code did not change is listed when a function it calls got slower.

The check runs when `gate.complexity_regression` is `warning` or `error`, or for one run
when `--regressions` is passed to `rqx check` or `rqx comment`. With the setting `off`
and no flag, no output changes.

| `gate.complexity_regression` | Effect |
|---|---|
| `off` (default) | Nothing is compared, unless `--regressions` is passed. Then the regressions are listed and the exit code does not change. |
| `warning` | The regressions are listed. The exit code does not change. |
| `error` | The regressions are listed, and any regression fails the run with exit code `1`. The verdict names the first three functions and counts the rest. |

```yaml
gate:
  complexity_regression: error
performance:
  max_order: O(n log n)
```

`performance.max_order` is a separate limit that needs no base revision. Any function in
the reviewed files whose time order is above it fails the run, and the verdict names the
function. A function whose cost the analysis marks `unknown` is not counted. `rqx scan`
applies the limit to every file when a gate flag turns its gate on.

**How the base is read.** The base revision is not scanned for this. The orders at the
head are the scan's own. The orders at the base are composed over the same project with
only the files the change touched read as they were at the base revision, from
`git show`. Every other file contributes the facts the head scan already holds. A change
to a callee therefore reaches its callers in files nobody edited.

**What counts as the same function.** A function is identified by its file and its name
qualified by its class. A renamed function, a function moved to another file and a
function added by the change are new, and are never reported as regressions. A function
that the base revision held with a constant cost, and that the change gave a loop, is
compared as `O(1)`. Overloads that share a qualified name and functions with no name are
not compared. A change that deletes a file is compared without it, so a caller of a
deleted function is costed as if the call were constant at the base.

**Output.**

| Output | Where the regressions appear |
|---|---|
| Terminal | A `Complexity` row in the summary, with the first three functions. |
| `rqx comment` | A `Complexity regressions` section above the folded detail. Each function is a line with its orders and location, and the trace, from the dominant loop down to the callee that sets the order, is folded under `Trace`. |
| JSON | A `regressions` array on the summary line. See [json.md](json.md). |
| SARIF | One `complexity-regression` result per function. |

```
### Complexity regressions

- **`reconcileBatch`** O(n) → O(n²) · `src/x.ts:90`

<details><summary>Trace</summary>

**`reconcileBatch`**

- loop · `src/x.ts:90` · O(n²)
- findMatch · `src/y.ts:12` · O(n)

</details>
```

**When nothing is compared.** The comment, the terminal and the JSON say why instead of
reporting an empty list. There is no comparison for a review of `--files-from` or
`--staged`, which name no base revision, or when the scan did not read the whole project,
because a call into a file the scan skipped would be costed as constant. A skipped
comparison never fails the run.

**Code scanning.** A SARIF run carries the regressions as results of the rule
`complexity-regression`. The level follows the gate: `error`, `warning`, or `note` for a
review that was asked with `--regressions` and has the gate off. A result is placed at its
function, which may be in a file the change did not touch, so GitHub shows it in the
Security tab and not as an annotation on the diff. The regressions exist only relative to
a base revision, so a full `rqx scan` upload never reports them. When both are uploaded
to the same category, the next full scan closes the alerts the review opened. Give the
review its own category with `RQX_SARIF_CATEGORY`, as described under
[SARIF and GitHub code scanning](#sarif-and-github-code-scanning).

## Commenting on the merge request

`rqx comment` posts the review on the merge request under review. It writes one
comment, which later runs edit in place, and a commit status that carries the grade.

```sh
rqx comment --fail-on error
```

The comment has a `Resolved` section when the change removed findings that the base
revision had. A finding counts as resolved when the base reports it and the working
tree does not, using the matching that `rqx verify` uses, so a finding that moved to
another file is not credited. The section lists the findings. `rqx check` reports the
same data in the terminal summary and in the `resolved`, `resolved_scope` and
`health_delta` keys of the JSON summary line.

What the credit covers depends on whether the base was scanned (see
[Functions that repeat existing code](#functions-that-repeat-existing-code)):

| `resolved_scope` | When | Covers | `health_delta` |
|---|---|---|---|
| `project` | The change adds a function, so the base was scanned whole. | Every finding of the base scan, including those that read more than one file. | The health score after the change minus the score at the base. |
| `changed-files` | The change adds no function. | The findings that one file's analysis produces, in the files the change touched or deleted. A finding that needs the whole project, such as `unused-export`, `unused-file`, `duplicate-code` or an import cycle, is not credited. | Absent: a score needs every finding of both sides. |

The base side of a `changed-files` credit is each touched file's text at the merge
base, read with `git show` and analysed on its own. The section is omitted when
`--max-findings` dropped findings from the working tree's scan, because a dropped
finding cannot be told from a resolved one.

It finds the merge request from the pipeline's own variables: GitLab's
`CI_MERGE_REQUEST_IID` and `CI_PROJECT_ID`, and GitHub's `GITHUB_REPOSITORY` and event
payload. Outside a merge request it exits `2` with an explanation.

`--base` defaults to the merge request's own base: GitLab's
`CI_MERGE_REQUEST_DIFF_BASE_SHA` or GitHub's `pull_request.base.sha`. Both are commits
present in the clone. `origin/HEAD` is not, because a CI clone does not set it. Pass
`--base` to override the default.

The commit that receives the grade is the head of the branch, not the merge commit the
runner built. On a `pull_request` event `GITHUB_SHA` names a commit that belongs to no
branch, so a status posted there would not be shown.

| Flag | Effect |
|---|---|
| `--dry-run` | Print the comment and its destination. Post nothing. |
| `--marker <id>` | Identify which comment to update, so two jobs can each keep their own. |
| `--no-status` | Post the comment and leave the commit without a grade. |
| `--base <ref>` | The revision the review compares against. |

**Tokens.** GitLab requires `RQX_TOKEN` or `GITLAB_TOKEN` with `api` scope.
`CI_JOB_TOKEN` can read merge request notes but not write them, which is a GitLab
restriction. GitHub requires `permissions: pull-requests: write`, and `statuses: write`
for the commit grade.

Requests go through `curl`. rqx already requires `git`, CI images generally include
`curl`, and using it avoids a TLS stack in a binary that has no runtime dependencies.
Certificates and proxies come from the system configuration. Alpine images need
`apk add curl`. Any curl version works. The token and payload are passed through a `0600`
configuration file and not on the command line, where `ps` would expose the token to
other jobs on the runner. The status code is read with `--write-out`, not with an
option that exists only from curl 7.76.

### GitLab

```yaml
rqx:
  image: alpine
  variables:
    GIT_DEPTH: 0
  before_script:
    - apk add --no-cache curl git
  script:
    - rqx comment --fail-on error
    - rqx scan . --reporter gitlab=gl-code-quality-report.json
  artifacts:
    reports:
      codequality: gl-code-quality-report.json
  rules:
    - if: $CI_PIPELINE_SOURCE == "merge_request_event"
```

`GIT_DEPTH: 0` is required for the same reason as `fetch-depth: 0` below. A review
diffs `base...HEAD`, and GitLab's default shallow clone may contain no commit that the
two branches share. In that case rqx reports the shallow clone by name and not a
missing revision.

The `gitlab` reporter supplies the inline part: GitLab reads it as a Code Quality
report and draws each finding on the diff, below the summary comment.

### GitHub

```yaml
permissions:
  contents: read
  pull-requests: write
  statuses: write

jobs:
  rqx:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with: { fetch-depth: 0 }
      - run: rqx comment --fail-on error
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

`fetch-depth: 0` is required because a review compares against a revision, and a
shallow clone does not contain one.
