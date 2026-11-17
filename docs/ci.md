# rqx in CI

One binary, one analysis pass, one exit code. Everything below is the same tool
with a different `--reporter`.

## The contract

| Exit code | Meaning |
|---|---|
| `0` | ran, gate satisfied |
| `1` | ran, gate failed |
| `2` | rqx could not run — bad path, bad config, unresolvable revision |
| `101` | rqx hit a bug; please report it |

A closed pipe is not a failure. `rqx scan . | head` exits `0`: the scan did what it was
asked, and the reader going away is the reader's decision, not a broken run.

`rqx scan` never fails on its own. It gates only when you pass one of the gate
flags. `rqx check` always gates, using `gate` from `rqx.yaml` unless a flag
overrides it.

| Flag | Fails when |
|---|---|
| `--fail-on <severity>` | any finding at or above that severity |
| `--allow-findings <n>` | more than `n` of them |
| `--min-health <n>` | the project score drops below `n` |
| `--min-grade <grade>` | any single file grades below that floor |

`--min-grade` is the one that catches a file rotting on its own: a project-wide
score averages it away, and severity is per finding rather than per file. Grades
run `pristine`, `clean`, `fair`, `poor`, `critical`, and `never` turns the gate off.

Passing any gate flag opts a scan into gating, so the rest of `[gate]` applies too
— `--min-grade poor` on a repo with `fail_on = "error"` gates on both.

`fail_on = "never"` turns off only the count of findings by severity. The health floor,
the grade floor, the ratchet and `fail_on_incomplete` still apply when they are set.

## Reporters

`--reporter <name>` writes to stdout. `--reporter <name>=<path>` writes to a file.
Both are repeatable, and one run can serve every consumer at once:

```sh
rqx scan . \
  --reporter terminal \
  --reporter junit=rqx.xml \
  --reporter sarif=rqx.sarif \
  --fail-on error
```

| Reporter | Consumer |
|---|---|
| `terminal` | the job log |
| `json` | newline-delimited JSON, one object per finding then a summary; [what it promises](json.md) |
| `sarif` | GitHub code scanning, Azure DevOps advanced security |
| `junit` | GitLab, Jenkins, CircleCI, Azure Pipelines, Bitbucket, Buildkite |
| `gitlab` | GitLab Code Quality — inline merge-request annotations |
| `github` | GitHub Actions annotations on the diff |
| `markdown` | job summaries, MR comments, chat |
| `html` | a report a person opens from a pipeline's artifacts; [below](#html-report) |

`terminal` and `github` can share stdout; anything else is a document and has to
have the stream, or a file, to itself.

Colour follows `NO_COLOR` and whether stdout is a terminal. `--color never`
forces it off, `--color always` forces it on for a log viewer that renders ANSI.

The progress bar draws only when stderr is a terminal, so a CI log never holds it.
`--no-progress`, or `RQX_NO_PROGRESS=1`, turns it off on a terminal too, and `--quiet`
implies it.

### Where the verdict lives

The exit code is the contract, but a report is often read somewhere the exit code
does not reach. A gated run writes its verdict into the document too:

| Reporter | Where to read it |
|---|---|
| `terminal` | the `gate passed` or `gate failed` line, with each failure under it |
| `json` | `gate` on the summary line: `{ "passed": bool, "failures": [...] }` |
| `junit` | a `<testsuite name="rqx gate">` with one test, failing when the gate fails; the totals on `<testsuites>` count it |
| `github` | the last annotation: `::error title=rqx gate::` with the failures, or `::notice title=rqx gate::gate passed` |
| `sarif` | `runs[0].invocations[0].exitCode`, one `toolExecutionNotifications` entry per failure at level `error`, and `runs[0].properties.rqx.gate` |

A scan that was not gated has no verdict, so these are absent rather than reading
`passed`. `rqx check` and `rqx comment` always have one.

### SARIF and GitHub code scanning

Code scanning matches the alerts in a new upload against the ones it already holds.
Two things change that matching, so both are off until you ask:

| Variable | Effect |
|---|---|
| `RQX_SARIF_FINGERPRINTS=1` | adds `partialFingerprints` to every result, built from the finding's identity (the same one the GitLab report uses), so an alert follows its finding when code above it moves |
| `RQX_SARIF_CATEGORY=<id>` | adds `automationDetails.id`, which files the run under that category |

Turning either on for a repository that already has open alerts re-keys them: GitHub
sees new fingerprints or a new category, closes the old alerts and opens the same
findings again. Set them once, deliberately, on a repository whose alert history you
can afford to reset. Set a category when one repository uploads several SARIF runs
that would otherwise overwrite each other.

### HTML report

`html` writes one self-contained file: the findings by family and rule, then every
file worst grade first with its code frames, and the gate's verdict when there is
one. It carries its own styles, loads nothing from the network and works without
JavaScript, so it opens from an artifact download as it is. It follows the system's
light or dark setting.

GitHub Actions:

```yaml
- run: rqx scan . --reporter terminal --reporter html=rqx.html --fail-on error
- uses: actions/upload-artifact@v4
  if: always()
  with:
    name: rqx-report
    path: rqx.html
```

GitLab CI, beside the other reports:

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
- uses: jadhamwi21/rqx@v1
  with:
    base: ${{ github.base_ref }}
    fail-on: error
```

The action runs the analysis once and produces the log, inline annotations, the
SARIF upload and a job summary from that single pass. It installs rqx by downloading
the binary its own tag was released with and checking it against `SHA256SUMS`; a ref
with no published binary — a fork, or `@main` ahead of the last release — compiles
from the checkout instead.

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

Fingerprints in the Code Quality report deliberately exclude the line number, so
editing the top of a file does not re-report everything below it as new. A file
holding the same finding more than once disambiguates the repeats by ordinal rather
than by position: GitLab shows one annotation per fingerprint, so without it the
second and third empty catch in a file would never reach the merge request.

## Running a job before you push

`gitlab-runner exec` was removed in GitLab 16, and `run-single` polls a real instance
for work, so neither runs a pipeline from a checkout. `tools/ci-local.sh` reads a
job's `script:` out of `.gitlab-ci.yml` and runs it here:

```bash
tools/ci-local.sh                 # the jobs in the file
tools/ci-local.sh lint            # run one
tools/ci-local.sh rqx --in ../app # run it against another checkout
```

It is not a runner — no image, no cache, no artifacts, no `rules:` — so a job that
passes locally can still fail in CI for a reason the container would have shown. What
it does check is the part that usually breaks: the commands, their order, and the exit
code.

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

Wrap the `sh` step in `catchError(buildResult: 'UNSTABLE', stageResult: 'FAILURE')`
if a failed gate should mark the build unstable rather than failed.

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

Bitbucket picks up anything under `test-reports/` automatically.

## Anywhere else

If a provider reads JUnit XML — and they all do — this works:

```sh
rqx scan . --reporter terminal --reporter junit=rqx.xml --fail-on error
```

If it does not, parse the NDJSON:

```sh
rqx scan . --reporter json \
  | jq -c 'select(.kind != "summary") | {file: .location.file, rule, severity}'
```

## Checking the config in CI

A typo in `rqx.yaml` stops a scan with one error at a time. `rqx config validate`
reports every problem in the config before any code is read:

```sh
rqx config validate --strict
```

An error exits 2, the same as a run that could not start. A warning, such as a rule
name that is not in the catalogue, exits 0 on its own and 1 under `--strict`, so a
pipeline that wants a clean config fails on it. `--json` prints one line per problem
with its file, line and column; see [json.md](json.md).

## Review without git history

`rqx check` normally asks git which files differ from `--base`. A shallow clone has no
common commit to diff from, and some pipelines already know which files changed. Pass
the list instead:

```sh
git diff -z --name-only origin/main... | rqx check --files-from -
```

`-z` separates the names with NUL, which `--files-from` detects, so a file name with a
space or a newline in it survives. Without `-z`, one path per line works too. The
list replaces the diff as the files under review. Whatever imports a listed file is
reviewed as well, and the whole project is still analysed, so a finding that depends
on another file stays correct. Paths that are not files under the scanned directory
are named once on stderr and skipped.

Any program can write the list, such as a changed-files API call or a build tool's
affected-projects output. `--base` and `--files-from` cannot be combined.

## Adopting a gate on an existing codebase

A gate that fails on day one gets disabled on day one. Start where you already
are and tighten from there:

```yaml
# rqx.yaml
gate:
  fail_on: error        # only the three rules that are never a false positive
  max_findings: 0
  min_health: 0
```

Then move to `rqx check --base main`, which reviews the changed files plus
everything that imports them, so the gate applies to new work rather than to the
history you inherited.

## Failing on a half-applied pattern

`[gate] fail_on_incomplete = true`, or `--fail-on-incomplete`, is the only way design
pattern advice reaches an exit code. It sees the half-applied stance and nothing else:
a decorator that cannot substitute for what it wraps, a subscribe with no unsubscribe.
A pattern that is merely in use, or would merely fit, can never fail a run, and none
of the advice moves the health score in any configuration. `rqx check` and
`rqx comment` apply it to the files they review; `rqx scan` applies it to all of them.

## Commenting on the merge request

`rqx comment` posts the review onto the merge request it is reviewing: one comment,
edited on every re-run rather than repeated, and a commit status carrying the grade
so the branch shows it without anyone opening a log.

```sh
rqx comment --fail-on error
```

It reads the pipeline's own variables to find the merge request — GitLab's
`CI_MERGE_REQUEST_IID` and `CI_PROJECT_ID`, GitHub's `GITHUB_REPOSITORY` and event
payload — and exits 2 with an explanation when it is not running on one.

**`--base` defaults to the merge request's own base**, which the pipeline already
knows: GitLab's `CI_MERGE_REQUEST_DIFF_BASE_SHA`, GitHub's
`pull_request.base.sha`. Both are commits that exist in the clone, unlike
`origin/HEAD`, which a CI clone leaves pointing at nothing. Pass `--base` to
override it.

The commit the grade lands on is the head of the branch, not the merge commit the
runner built: on a `pull_request` event `GITHUB_SHA` names a commit that belongs to
no branch, so a status posted there would show on nothing.

| Flag | Does |
|---|---|
| `--dry-run` | print the comment and where it would go; post nothing |
| `--marker <id>` | which comment to keep, so two jobs each keep their own |
| `--no-status` | comment, but leave the commit ungraded |
| `--base <ref>` | what the review compares against |

**Tokens.** GitLab needs `RQX_TOKEN` or `GITLAB_TOKEN` with `api` scope —
`CI_JOB_TOKEN` can read merge-request notes but not write them, which is a GitLab
limitation and not something rqx can work around. GitHub needs
`permissions: pull-requests: write`, and `statuses: write` for the commit grade.

Requests go through `curl`. rqx already requires `git`; every CI image has `curl`,
and the alternative is a TLS stack in a binary whose claim is that it has no runtime
dependencies. Certificates and proxies stay the system's, which on a self-hosted
GitLab is where they belong. Alpine images need `apk add curl`. Any curl will do —
the token and the payload go through a `0600` config file rather than the command
line, where `ps` would show the token to every other job on the runner, and the
status code comes back through `--write-out` rather than through an option that
only exists from 7.76.

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

`GIT_DEPTH: 0` matters for the same reason `fetch-depth: 0` does below: a review
diffs `base...HEAD`, and GitLab's default shallow clone may hold no commit the two
branches share. rqx says so by name when it happens rather than reporting a missing
revision.

The `gitlab` reporter is the other half: GitLab reads it as a Code Quality report and
draws each finding inline on the diff, where the comment is the summary above it.

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

`fetch-depth: 0` matters: a review compares against a revision, and a shallow clone
does not have one.
