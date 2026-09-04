# The rule catalogue

Every rule rqx can report, what it measures, and the named refactoring that answers it. Each one cites [refactoring.guru](https://refactoring.guru/refactoring/smells) or a numbered heuristic from *Clean Code* chapter 17, because a finding that cannot say where it comes from is an opinion.

`rqx explain <rule>` prints any of these at the terminal. [Configuration](../configuration.md) covers the file that turns them on and off.

| Family | Rules | |
|---|---|---|
| [Bloaters](bloaters.md) | 8 | `long-function` `cognitive-complexity` `deep-nesting` `long-parameter-list` `large-class` `data-clump` `primitive-obsession` `obscured-expression` |
| [Object-Orientation Abusers](oo-abusers.md) | 7 | `type-code-switch` `temporary-field` `refused-bequest` `base-depends-on-derived` `inappropriate-static` `temporal-coupling` `alternative-classes` |
| [Change Preventers](change-preventers.md) | 3 | `shotgun-surgery` `divergent-change` `parallel-inheritance` |
| [Dispensables](dispensables.md) | 18 | `duplicate-code` `unused-export` `unused-file` `unused-parameter` `unused-local` `unused-private-member` `unreachable-code` `commented-out-code` `lazy-class` `data-class` `speculative-generality` `empty-catch` `magic-number` `mixed-language-file` `distant-declaration` `buried-configuration` `journal-comment` `redundant-comment` |
| [Couplers](couplers.md) | 5 | `circular-dependency` `message-chain` `feature-envy` `middle-man` `inappropriate-intimacy` |
| [Naming (Clean Code ch. 2)](naming.md) | 7 | `mixed-conventions` `vague-function-name` `misleading-pattern-name` `inconsistent-vocabulary` `meaningless-name` `short-name-wide-scope` `hungarian-notation` |
| [Safety (Clean Code ch. 17 general)](safety.md) | 8 | `imprecise-type` `boundary-arithmetic` `query-with-side-effect` `suppressed-diagnostic` `flag-argument` `output-parameter` `complex-condition` `negative-conditional` |
| [Tests (Clean Code ch. 9)](tests.md) | 4 | `slow-test` `skipped-test` `focused-test` `assertionless-test` |

60 rules in total. 10 ship **off by default** because their signal is real but noisy; name one under `[rules] enabled` to turn it on:

- `primitive-obsession`
- `divergent-change`
- `parallel-inheritance`
- `misleading-pattern-name`
- `inconsistent-vocabulary`
- `alternative-classes`
- `meaningless-name`
- `short-name-wide-scope`
- `hungarian-notation`
- `negative-conditional`
