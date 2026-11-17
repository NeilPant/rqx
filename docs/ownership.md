# Ownership

rqx reads a CODEOWNERS file and attaches the owners to every finding. From that it
reports the health of each owner's files, groups the listing by owner, names owners in
merge request comments and applies a gate per owner. A project without a CODEOWNERS
file sees none of this, and its output is unchanged.

## Where the file is read from

rqx looks in the scanned directory, in this order, and uses the first file that holds
at least one rule:

1. `.github/CODEOWNERS`
2. `.gitlab/CODEOWNERS`
3. `CODEOWNERS`
4. `docs/CODEOWNERS`

A file under `.github/` is read as GitHub's format. One under `.gitlab/` is read as
GitLab's. A file at the root or in `docs/` is read as GitLab's when it has a `[Section]`
header, and as GitHub's otherwise.

Paths in the file are relative to the scanned directory. Rules are read in order, and
for a given path the last matching rule decides the owners.

## Patterns

Patterns follow the `.gitignore` rules, with the differences GitHub documents:

- A pattern with a slash before its last character matches from the root. One without
  matches at any depth.
- A trailing slash matches a directory and everything under it.
- `*` and `?` match within one path component. `docs/*` owns the files directly in
  `docs/`, not those in `docs/a/`.
- A component of `**` matches any number of directories.
- `!` negation and `[...]` character classes are not supported. A rule that starts with
  `!` is ignored.

In GitLab's format, a `[Section]` header groups rules, `^[Section]` marks the section
optional and `[Section][2]` requires two approvals. A header may list default owners
for the rules under it that name none. Within a section the last matching rule decides.
A path takes the owners of every section that has a matching rule. Section names are
not case sensitive, and two headers with the same name are one section.

A rule that names no owners, outside a section with defaults, leaves the path without
an owner.

## Owners on findings

`rqx scan --reporter json` and `rqx check --reporter json` add an `owners` array to each
finding line, and an `owners` object to the summary line, when a CODEOWNERS file
exists:

```json
{"kind":"finding","rule":"long-function","owners":["@web","@lead"], "...": "..."}
{"kind":"summary","owners":{"@web":{"files":12,"findings":31,"health":58,"grade":"poor"}}}
```

The array is empty for a file no rule names. In the summary, those files are listed under
the key `(unowned)`. See [json.md](json.md) for the compatibility rules.

## Health per owner

An owner's health is computed with the project's own formula, over the files the owner
owns: the weighted findings in those files, scaled by each file's impact, against the
lines of code in them. The number is comparable with the project's health. A file with
several owners counts in full for each of them, so the owners' file counts can add up
to more than the project's.

Each row has:

| Column | Meaning |
|---|---|
| `files` | Scanned files the owner owns |
| `findings` | Findings in those files, before any display cap |
| `health` | The health score of those files, 0 to 100 |
| `grade` | The grade of that score: `pristine`, `clean`, `fair`, `poor` or `critical` |

Rows are sorted by health, lowest first, then by findings, then by name. Owner health
always covers the whole project, including in `rqx check`, so a review cannot raise it
by looking at fewer files.

The table appears:

- in the terminal summary of `rqx scan` and `rqx check`, after the totals;
- in the HTML report, as an Owners section after the summary;
- in the JSON summary, as the `owners` object;
- from `rqx owners`.

## `rqx owners`

```sh
rqx owners [PATH] [--reporter terminal|json] [--json]
```

Scans `PATH` and prints the table, one row per owner. With `--reporter json` or `--json`
it prints one object per owner:

```json
{"owner":"@web","files":12,"findings":31,"health":58,"grade":"poor"}
```

Without a CODEOWNERS file the terminal form lists the places rqx looked and the JSON
form prints nothing. In the interactive session, `/owners` prints the same output for
the held scan.

## `--group-by owner`

`rqx scan --group-by owner` and `rqx check --group-by owner` list findings under the
owners of their file instead of under the file. A finding in a file with several owners
is listed under each. Findings in files no rule names are listed under `(unowned)`.
Without a CODEOWNERS file the report says so and lists every finding as unowned.
Groups are ordered by finding count, and `--top N` limits how many are listed.

## Owners in `rqx comment`

When a CODEOWNERS file exists, the comment gains a line after the summary:

```
Owners of files with new findings: @web (3 findings), @api (1 finding).
```

It counts findings the baseline does not already hold, in the files the comment covers.
Files no rule names are left out.

By default the owners are written inside code spans, as `@web`, which GitHub and GitLab
do not turn into mentions, so nobody is notified. Setting `comment.mention_owners` in
`rqx.yaml` writes them as bare mentions, which the forge notifies as it would for a
review request:

```yaml
comment:
  mention_owners: true
```

## A gate per owner

`gate.owners` in `rqx.yaml` maps an owner, written as in the CODEOWNERS file, to the
floor that owner's files must meet:

```yaml
gate:
  owners:
    "@team-a": { min_health: 70, fail_on: error }
    "@team-b": { min_health: 50 }
```

| Key | Default | Meaning |
|---|---|---|
| `min_health` | `0` | Fails when the owner's health is below this value. `0` disables the check. |
| `fail_on` | `never` | Fails when the owner's files in scope hold a finding at or above this severity: `error`, `warning`, `info` or `never`. Findings a baseline records are not counted. |

The checks run wherever the gate does: `rqx check`, `rqx comment`, and `rqx scan` when a
gate flag is passed. `min_health` uses the owner's whole-project health. `fail_on`
counts findings in the files the verdict covers, which for `rqx check` are the files
the branch changed.

Each owner that falls short adds a line to the verdict:

```
`@team-a` health score 62 is below the required 70
`@team-a` has 3 findings at or above `error`
```

Owner names are matched without regard to case. An owner that owns no scanned file has
nothing to fail on. If `gate.owners` is set and the project has no CODEOWNERS file, the
gate fails with a message saying so, since there is nothing to check the floors against.
The gate does not change when `gate.owners` is empty or absent.
