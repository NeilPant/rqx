# The rule catalogue

Every rule rqx can report, what it measures, and the named refactoring that answers it. Each one names the metric that decides it, because a finding that cannot say where it comes from is an opinion.

`rqx explain <rule>` prints any of these at the terminal. [Configuration](../configuration.md) covers the file that turns them on and off.

| Family | Rules | |
|---|---|---|
| [Size](size.md) | 12 | `long-function` `cognitive-complexity` `deep-nesting` `long-parameter-list` `large-class` `data-clump` `primitive-obsession` `obscured-expression` `many-exits` `wide-switch` `callback-nesting` `oversized-module` |
| [Abstraction](abstraction.md) | 8 | `type-code-switch` `temporary-field` `refused-bequest` `base-depends-on-derived` `inappropriate-static` `temporal-coupling` `alternative-classes` `wide-surface` |
| [Change](change.md) | 3 | `shotgun-surgery` `divergent-change` `parallel-inheritance` |
| [Waste](waste.md) | 22 | `duplicate-code` `unused-export` `unused-file` `unused-parameter` `unused-local` `unused-private-member` `unreachable-code` `commented-out-code` `lazy-class` `data-class` `speculative-generality` `empty-catch` `magic-number` `mixed-language-file` `distant-declaration` `buried-configuration` `journal-comment` `redundant-comment` `cross-language-duplicate` `empty-method` `discarded-expression` `duplicate-import` |
| [Coupling](coupling.md) | 10 | `circular-dependency` `layer-violation` `forbidden-import` `message-chain` `feature-envy` `middle-man` `inappropriate-intimacy` `deep-relative-import` `wide-fan-out` `wide-fan-in` |
| [Flow](flow.md) | 14 | `redundant-else` `collapsible-if` `boolean-from-branch` `literal-comparison` `self-comparison` `identical-branches` `empty-block` `nested-ternary` `ambiguous-precedence` `negated-comparison` `no-default-branch` `loop-runs-once` `redundant-jump` `tiny-switch` |
| [Naming](naming.md) | 7 | `mixed-conventions` `vague-function-name` `misleading-pattern-name` `inconsistent-vocabulary` `meaningless-name` `short-name-wide-scope` `hungarian-notation` |
| [Safety](safety.md) | 11 | `imprecise-type` `boundary-arithmetic` `query-with-side-effect` `assignment-in-condition` `float-equality` `unjustified-suppression` `suppressed-diagnostic` `flag-argument` `output-parameter` `complex-condition` `negative-conditional` |
| [Tests](tests.md) | 4 | `slow-test` `skipped-test` `focused-test` `assertionless-test` |
| [Noise](noise.md) | 5 | `banner-comment` `chat-residue` `apology-comment` `emoji-in-source` `example-value` |
| [Performance](performance.md) | 8 | `scan-in-loop` `deep-loop-nesting` `query-in-loop` `await-in-loop` `invariant-in-loop` `accumulate-by-copy` `branching-recursion` `blocking-call-in-async` |

104 rules in total. 13 ship **off by default** because their signal is real but noisy; name one under `[rules] enabled` to turn it on:

- `primitive-obsession`
- `divergent-change`
- `parallel-inheritance`
- `misleading-pattern-name`
- `inconsistent-vocabulary`
- `alternative-classes`
- `cross-language-duplicate`
- `meaningless-name`
- `short-name-wide-scope`
- `hungarian-notation`
- `tiny-switch`
- `emoji-in-source`
- `negative-conditional`
