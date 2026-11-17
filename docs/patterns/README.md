# Design patterns

How rqx treats the twenty-three patterns in *Design Patterns*. It reports three kinds of result, which do not rank: a pattern in use is inventory, a pattern half applied is a defect, and a pattern that would fit is a suggestion about a shape already in the code.

Pattern results do not change the health score. A pattern that would fit is not a finding. The only route to an exit code is `gate.fail_on_incomplete`, which considers only the half-applied patterns.

Run `rqx patterns` for a project, `rqx patterns --coverage` for this table, and `rqx explain <pattern>` for any one of them.

| Family | Patterns | Advisories |
|---|---|---|
| [Creational](creational.md) | 5 | 7 |
| [Structural](structural.md) | 7 | 8 |
| [Behavioural](behavioural.md) | 11 | 12 |

23 patterns, 23 implemented, 27 advisories.

