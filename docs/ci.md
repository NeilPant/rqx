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

Enabling either variable on a repository that already has open alerts re-keys them.
GitHub sees new fingerprints or a new category, closes the old alerts and opens the
same findings again. Set a category when one repository uploads several SARIF runs
that would otherwise overwrite each other.

### HTML report

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
- uses: jadhamwi21/rqx@v0.2.1
  with:
    base: ${{ github.base_ref }}
    fail-on: error
```

The action runs the analysis once and produces the log, inline annotations, the SARIF
upload and a job summary from that pass. It installs rqx by downloading the binary
released with its own tag and verifying it against `SHA256SUMS`. A ref with no
published binary, such as a fork or `@main` ahead of the last release, compiles rqx
from the checkout instead.

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
(`prefix: rqx-0.2.1`). The cache only affects speed. A missing, outdated or damaged
file is rebuilt on the next scan, and no report depends on it.

## GitLab CI

```yaml
rqx:
  image: rust:latest
  script:
    - cargo install --locked --git https://github.com/jadhamwi21/rqx rqx
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
- run: cargo install --locked --git https://github.com/jadhamwi21/rqx rqx
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

`rqx check` and `rqx comment` also scan the revision the change forked from, the merge
base of `--base` and `HEAD`. A function that exists only in the working tree and whose
body matches a function that exists at both revisions is reported as
`reimplemented-existing`. The finding names the existing function and its location.

| Match | Confidence |
|---|---|
| The same normalised syntax, differing only in local names. | `verified` |
| The same control flow and constants in one language, with other member names, in a body of at least 8 statements and 40 syntax nodes. | `heuristic` |
| The same control flow and constants in a file of another language, with the same size floor. | `low` |

A function that was renamed or moved is not reported, because the function it replaced
no longer exists to call. `clone_min_statements` and `clone_min_nodes` set the smallest
body that is compared. The base scan roughly doubles the run time of `rqx check`, and
the base checkout starts without a scan cache. `--files-from` names no revision, so
the rule does not run there, and `rqx scan` never reports it.

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

## Commenting on the merge request

`rqx comment` posts the review on the merge request under review. It writes one
comment, which later runs edit in place, and a commit status that carries the grade.

```sh
rqx comment --fail-on error
```

The comment has a `Resolved` section when the change removed findings that the base
revision had. It lists them and states the health score at the base and after the
change. A finding counts as resolved when the base scan reports it and the scan of the
working tree does not, using the matching that `rqx verify` uses, so a finding that
moved to another file is not credited. The section is omitted when `--max-findings`
dropped findings from either scan, because a dropped finding cannot be told from a
resolved one. `rqx check` reports the same data in the terminal summary and in the
`resolved` and `health_delta` keys of the JSON summary line.

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
