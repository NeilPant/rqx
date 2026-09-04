# Design patterns

What Rqx makes of the twenty-three patterns in *Design Patterns*. Three things are being said, and they are not degrees of one another: a pattern **in use** is inventory, one **half applied** is a defect, and one that **would fit** is a suggestion about a shape that is already there.

**None of it moves the health score.** A pattern that would fit is not a finding, and a score that fell because code declined to use Strategy would stop meaning anything. The one route to an exit code is `[gate] fail_on_incomplete`, which sees only the half-applied half.

Run `rqx patterns` for a project, `rqx patterns --coverage` for this table, and `rqx explain <pattern>` for any one of them.

| Family | Patterns | Advisories |
|---|---|---|
| [Creational](creational.md) | 5 | 7 |
| [Structural](structural.md) | 7 | 8 |
| [Behavioural](behavioural.md) | 11 | 12 |

23 patterns, 23 implemented, 27 advisories.

