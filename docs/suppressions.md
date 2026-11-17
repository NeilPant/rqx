# Waivers

A waiver is a source comment that excludes one finding from the results. It is used
when a finding is a false positive, or is correct and accepted. It is narrower than
disabling the rule for the whole project.

```ts
// rqx-ignore long-function: the parser is generated, PLAT-812
export function parse(input: string): Ast {
```

A waiver applies to findings on its own line and on the next line, so a comment
directly above a declaration covers that declaration. It works in any comment syntax
rqx parses: `//`, `#`, and `/* */`.

## The reason is not optional

```ts
// rqx-ignore long-function
```

A waiver without a reason has no effect. The finding is still reported, and rqx
reports the comment itself:

```
  ● unjustified-suppression  parse.ts:41  warning
    rqx-ignore with no reason
    Why:  the waiver is refused and the finding still stands, because a waiver
          nobody has to justify is how a rule gets switched off one line at a time
```

Requiring a reason keeps every exemption documented in the source.

## Forms

| Form | What it waives |
|---|---|
| `rqx-ignore long-function: <reason>` | that rule, on this line and the next |
| `rqx-ignore long-function, deep-nesting: <reason>` | either rule |
| `rqx-ignore size: <reason>` | any rule in the `size` family |
| `rqx-ignore all: <reason>` | every rule |
| `rqx-ignore-file circular-dependency: <reason>` | that rule, anywhere in the file |

A cycle, a dead export and a churn finding are determined across files, so they are
reported at line 1, and only the `-file` form waives them.

## Putting a date on it

```python
# rqx-ignore magic-number until 2027-01-01: the tariff table lands in Q1
```

After that date the waiver is no longer applied, as if the reason had been left out.
The finding is reported again, and `unjustified-suppression` reports the comment as
expired. A waiver with no date stays in effect until it is deleted.

## Writing one from the command line

```sh
rqx waive src/parse.ts:41 long-function --reason "the parser is generated, PLAT-812"
rqx waive src/tariff.py:88 magic-number --reason "the tariff table lands in Q1" --until 2027-01-01
rqx waive src/generated.ts circular-dependency --reason "generated" --whole-file
```

`rqx waive` inserts the comment above the line, with the same indentation, in the
comment style of the file's language. It refuses a rule that does not exist, and
names the closest rule. It also refuses a blank reason. A scan would not apply a
waiver with either problem. `--dry-run` prints the diff and changes nothing. The
editor command `rqx.suppress` writes the same comment.

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

`--expiring` shows only the waivers that carry a date. `--json` writes one object per
waiver.

Waivers are reported outside this command as well. The scan summary counts them, the
JSON summary carries `suppressed`, and SARIF reports each one as a result with
`suppressions: [{ "kind": "inSource", "justification": … }]`, so a code scanning
interface shows it as waived.

## What a waiver is not

A waived finding does not count towards the health score, the grade or the gate. To
record findings that are accepted as existing debt, use a [baseline](baseline.md)
instead.
