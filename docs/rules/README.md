# The rule catalogue

Every rule rqx can report, what it measures, and the named refactoring that answers it. Each one names the metric that decides it, because a finding that cannot say where it comes from is an opinion.

`rqx explain <rule>` prints any of these at the terminal. [Configuration](../configuration.md) covers the file that turns them on and off.

| Family | Rules | |
|---|---|---|
| [Size](size.md) | 14 | `long-function` `cognitive-complexity` `deep-nesting` `long-parameter-list` `large-class` `data-clump` `primitive-obsession` `obscured-expression` `many-exits` `wide-switch` `callback-nesting` `many-locals` `wide-try` `oversized-module` |
| [Abstraction](abstraction.md) | 11 | `type-code-switch` `temporary-field` `refused-bequest` `base-depends-on-derived` `inappropriate-static` `temporal-coupling` `alternative-classes` `wide-surface` `empty-subclass` `static-only-class` `working-constructor` |
| [Change](change.md) | 3 | `shotgun-surgery` `divergent-change` `parallel-inheritance` |
| [Waste](waste.md) | 27 | `duplicate-code` `unused-export` `unused-file` `unused-parameter` `unused-local` `unused-private-member` `unreachable-code` `commented-out-code` `lazy-class` `data-class` `speculative-generality` `empty-catch` `magic-number` `mixed-language-file` `distant-declaration` `buried-configuration` `journal-comment` `redundant-comment` `cross-language-duplicate` `empty-method` `discarded-expression` `unused-import` `rethrow-only-catch` `placeholder-body` `passthrough-function` `debug-output` `duplicate-import` |
| [Coupling](coupling.md) | 11 | `circular-dependency` `layer-violation` `forbidden-import` `message-chain` `feature-envy` `middle-man` `inappropriate-intimacy` `deep-relative-import` `wildcard-import` `wide-fan-out` `wide-fan-in` |
| [Flow](flow.md) | 14 | `redundant-else` `collapsible-if` `boolean-from-branch` `literal-comparison` `self-comparison` `identical-branches` `empty-block` `nested-ternary` `ambiguous-precedence` `negated-comparison` `no-default-branch` `loop-runs-once` `redundant-jump` `tiny-switch` |
| [Naming](naming.md) | 9 | `mixed-conventions` `vague-function-name` `misleading-pattern-name` `inconsistent-vocabulary` `meaningless-name` `short-name-wide-scope` `hungarian-notation` `negated-name` `shadows-builtin` |
| [Safety](safety.md) | 17 | `imprecise-type` `boundary-arithmetic` `query-with-side-effect` `assignment-in-condition` `float-equality` `mixed-return` `exit-from-finally` `catch-all-handler` `generic-throw` `library-exit` `boolean-literal-argument` `unjustified-suppression` `suppressed-diagnostic` `flag-argument` `output-parameter` `complex-condition` `negative-conditional` |
| [Tests](tests.md) | 8 | `slow-test` `branching-test` `duplicate-assertion` `hand-rolled-throws` `debug-output-in-test` `skipped-test` `focused-test` `assertionless-test` |
| [Noise](noise.md) | 5 | `banner-comment` `chat-residue` `apology-comment` `emoji-in-source` `example-value` |
| [Performance](performance.md) | 8 | `scan-in-loop` `deep-loop-nesting` `query-in-loop` `await-in-loop` `invariant-in-loop` `accumulate-by-copy` `branching-recursion` `blocking-call-in-async` |

127 rules in total. 14 ship **off by default** because their signal is real but noisy; name one under `[rules] enabled` to turn it on:

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
- `debug-output`
- `emoji-in-source`
- `negative-conditional`
