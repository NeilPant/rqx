# Waivers

Sometimes rqx is wrong, and sometimes it is right and the answer is still "not this
one". Turning the rule off for the whole project is the wrong size of hammer, so
there is a comment for it.

```ts
// rqx-ignore long-function: the parser is generated, PLAT-812
export function parse(input: string): Ast {
```

The waiver applies to findings on its own line and the next one, which is what a
comment directly above a declaration covers. It works in any comment syntax rqx
parses: `//`, `#`, and `/* */`.

## The reason is not optional

```ts
// rqx-ignore long-function
```

That waives nothing. The finding stands, and rqx reports the comment:

```
  ● unjustified-suppression  parse.ts:41  warning
    rqx-ignore with no reason
    Why:  the waiver is refused and the finding still stands, because a waiver
          nobody has to justify is how a rule gets switched off one line at a time
```

This is the whole point of the feature. A suppression mechanism with no cost is how
a codebase ends up with four hundred silent exemptions and no idea why any of them
are there.

## Forms

| Form | What it waives |
|---|---|
| `rqx-ignore long-function: <reason>` | that rule, on this line and the next |
| `rqx-ignore long-function, deep-nesting: <reason>` | either rule |
| `rqx-ignore size: <reason>` | any rule in the `size` family |
| `rqx-ignore all: <reason>` | every rule |
| `rqx-ignore-file circular-dependency: <reason>` | that rule, anywhere in the file |

A cycle, a dead export and a churn finding are decided across files, so they anchor
at line 1 and only the `-file` form reaches them.

## Putting a date on it

```python
# rqx-ignore magic-number until 2027-01-01: the tariff table lands in Q1
```

Past that date the waiver stops being honoured, exactly as if the reason had been
left out: the finding comes back and `unjustified-suppression` reports the comment
as expired. A waiver with no date lasts until someone deletes it.

## Writing one from the command line

```sh
rqx waive src/parse.ts:41 long-function --reason "the parser is generated, PLAT-812"
rqx waive src/tariff.py:88 magic-number --reason "the tariff table lands in Q1" --until 2027-01-01
rqx waive src/generated.ts circular-dependency --reason "generated" --whole-file
```

`rqx waive` inserts the comment above the line, indented like it, in the comment
style of the file's language. It refuses a rule that does not exist, naming the
closest one, and a blank reason, because the scan would not honour either.
`--dry-run` prints the diff and changes nothing. The editor's `rqx.suppress` command
writes the same comment.

## Reading them back

```sh
rqx suppressions
```

```
  Waivers  3 findings

  ● long-function  src/parse.ts:41
    the parser is generated, PLAT-812
    Jad Alhamwi, 2026-03-14

  ● magic-number  src/tariff.py:88
    the tariff table lands in Q1
    until 2027-01-01  ·  Ada Lovelace, 2026-08-02
```

`--expiring` shows only the ones carrying a date, which is the list worth reading
before a release. `--json` gives one object per waiver.

Waivers are visible everywhere, not just in that command. The scan summary counts
them, the JSON summary carries `suppressed`, and SARIF reports each one as a result
with `suppressions: [{ "kind": "inSource", "justification": … }]` — so a code
scanning UI shows it as waived rather than never seeing it at all.

## What a waiver is not

A waived finding does not count towards the health score, the grade, or the gate. If
what you want is "this is real debt we have not got to", that is a
[baseline](baseline.md), not a waiver.
