# The rule catalogue

Every rule rqx can report, what it measures, and the named refactoring that answers it. Each rule declares the metric that decides it.

`rqx explain <rule>` prints any of these at the terminal. [Configuration](../configuration.md) covers the file that turns them on and off.

| Family | Rules | |
|---|---|---|
| [Size](size.md) | 16 | `long-function` `cognitive-complexity` `deep-nesting` `long-parameter-list` `large-class` `data-clump` `primitive-obsession` `obscured-expression` `many-exits` `wide-switch` `callback-nesting` `cyclomatic-complexity` `many-results` `many-locals` `wide-try` `oversized-module` |
| [Abstraction](abstraction.md) | 17 | `type-code-switch` `temporary-field` `refused-bequest` `base-depends-on-derived` `inappropriate-static` `temporal-coupling` `alternative-classes` `wide-surface` `abstract-nothing` `split-cohesion` `method-ignores-instance` `assign-once-field` `single-method-field` `deep-inheritance` `empty-subclass` `static-only-class` `working-constructor` |
| [Change](change.md) | 7 | `hotspot` `single-author-file` `shotgun-surgery` `divergent-change` `parallel-inheritance` `stale-todo` `orphan-todo` |
| [Waste](waste.md) | 34 | `duplicate-code` `reimplemented-existing` `unused-export` `unused-file` `unused-parameter` `unused-local` `unused-private-member` `unreachable-code` `commented-out-code` `lazy-class` `data-class` `speculative-generality` `empty-catch` `magic-number` `mixed-language-file` `distant-declaration` `buried-configuration` `journal-comment` `redundant-comment` `cross-language-duplicate` `return-of-fresh-local` `empty-method` `barrel-module` `passthrough-override` `duplicate-function` `dead-store` `never-reassigned` `discarded-expression` `unused-import` `rethrow-only-catch` `placeholder-body` `passthrough-function` `debug-output` `duplicate-import` |
| [Coupling](coupling.md) | 18 | `circular-dependency` `layer-violation` `forbidden-import` `message-chain` `feature-envy` `middle-man` `inappropriate-intimacy` `global-accessor` `many-collaborators` `deep-relative-import` `mutable-module-state` `ambient-dependency` `self-built-collaborators` `import-time-work` `reaches-into-private` `wildcard-import` `wide-fan-out` `wide-fan-in` |
| [Flow](flow.md) | 17 | `redundant-else` `collapsible-if` `boolean-from-branch` `literal-comparison` `self-comparison` `identical-branches` `empty-block` `duplicate-condition` `silent-fallthrough` `nested-ternary` `ambiguous-precedence` `negated-comparison` `no-default-branch` `loop-runs-once` `redundant-jump` `tiny-switch` `nested-then` |
| [Naming](naming.md) | 12 | `mixed-conventions` `vague-function-name` `misleading-pattern-name` `inconsistent-vocabulary` `meaningless-name` `short-name-wide-scope` `hungarian-notation` `negated-name` `shadows-builtin` `boolean-not-a-question` `name-repeats-type` `plural-mismatch` |
| [Safety](safety.md) | 28 | `imprecise-type` `boundary-arithmetic` `query-with-side-effect` `assignment-in-condition` `closure-over-loop-variable` `mutated-while-iterated` `production-assert` `sleep-in-production` `shadowed-handler` `dropped-promise` `guard-cascade` `blanket-try` `no-base-case` `integer-division` `discarded-error` `float-equality` `mixed-return` `exit-from-finally` `catch-all-handler` `generic-throw` `library-exit` `boolean-literal-argument` `unjustified-suppression` `suppressed-diagnostic` `flag-argument` `output-parameter` `complex-condition` `negative-conditional` |
| [Tests](tests.md) | 18 | `slow-test` `real-world-test` `unlabelled-assertions` `self-asserting-test` `unexplained-expected-value` `empty-test` `eager-test` `string-shape-assertion` `unused-fixture` `unchecked-stub` `branching-test` `duplicate-assertion` `hand-rolled-throws` `debug-output-in-test` `unnamed-test` `skipped-test` `focused-test` `assertionless-test` |
| [Noise](noise.md) | 9 | `comment-per-line` `explanation-longer-than-code` `docstring-restates-signature` `over-logging` `banner-comment` `chat-residue` `apology-comment` `emoji-in-source` `example-value` |
| [Performance](performance.md) | 25 | `loop-concatenation` `sequential-await` `scan-in-loop` `deep-loop-nesting` `query-in-loop` `await-in-loop` `invariant-in-loop` `accumulate-by-copy` `branching-recursion` `blocking-call-in-async` `linear-op-in-loop` `sort-in-loop` `allocation-in-loop` `missing-capacity` `lock-in-loop` `unbounded-cache` `defer-in-loop` `boxing-in-loop` `copy-in-range-for` `large-by-value` `unbounded-concurrency` `quadratic-via-call` `deep-recursion` `repeated-computation-in-loop` `pair-scan-could-hash` |

201 rules in total. 24 ship **off by default** because their signal is real but noisy; name one under `rules.enabled` to turn it on:

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
- `real-world-test`
- `unlabelled-assertions`
- `unexplained-expected-value`
- `method-ignores-instance`
- `eager-test`
- `assign-once-field`
- `reaches-into-private`
- `debug-output`
- `emoji-in-source`
- `negative-conditional`
- `allocation-in-loop`
- `missing-capacity`
- `pair-scan-could-hash`
