# rqx in CI

One binary, one analysis pass, one exit code. Everything below is the same tool
with a different `--reporter`.

## The contract

| Exit code | Meaning |
|---|---|
| `0` | ran, gate satisfied |
| `1` | ran, gate failed |
| `2` | rqx could not run — bad path, bad config, unresolvable revision |

`rqx scan` never fails on its own. It gates only when you pass one of the gate
flags. `rqx check` always gates, using `[gate]` from `rqx.toml` unless a flag
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
| `json` | newline-delimited JSON, one object per finding then a summary |
| `sarif` | GitHub code scanning, Azure DevOps advanced security |
| `junit` | GitLab, Jenkins, CircleCI, Azure Pipelines, Bitbucket, Buildkite |
| `gitlab` | GitLab Code Quality — inline merge-request annotations |
| `github` | GitHub Actions annotations on the diff |
| `markdown` | job summaries, MR comments, chat |

`terminal` and `github` can share stdout; anything else is a document and has to
have the stream, or a file, to itself.

Colour follows `NO_COLOR` and whether stdout is a terminal. `--color never`
forces it off, `--color always` forces it on for a log viewer that renders ANSI.

## GitHub Actions

```yaml
- uses: actions/checkout@v4
  with:
    fetch-depth: 0          # `check` diffs against a base
- uses: jadhamwi/rqx@v1
  with:
    base: ${{ github.base_ref }}
    fail-on: error
```

The action runs the analysis once and produces the log, inline annotations, the
SARIF upload and a job summary from that single pass.

## GitLab CI

```yaml
rqx:
  image: rust:latest
  script:
    - cargo install --locked --git https://github.com/jadhamwi/rqx rqx
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
editing the top of a file does not re-report everything below it as new.

## CircleCI

```yaml
- run: cargo install --locked --git https://github.com/jadhamwi/rqx rqx
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

## Adopting a gate on an existing codebase

A gate that fails on day one gets disabled on day one. Start where you already
are and tighten from there:

```toml
# rqx.toml
[gate]
fail_on = "error"        # only the three rules that are never a false positive
max_findings = 0
min_health = 0
```

Then move to `rqx check --base main`, which reviews the changed files plus
everything that imports them, so the gate applies to new work rather than to the
history you inherited.
