# Tests

[Back to the catalogue](README.md)

## `slow-test`

a suite slow enough to skip stops being run, and a test nobody runs is worth less than the time it took to write

| | |
|---|---|
| Measures | a sleep or timeout inside a test body above the configured pause |
| Reported as | warning, verified |
| Reads | resolved scopes and symbols |
| Fix | Replace Algorithm |
| Tuned by | `thresholds.slow_test_ms` |

From `fixtures/typescript/suite.test.ts`, line 12:

```ts
it("waits on the clock", async () => {
  await new Promise((resolve) => setTimeout(resolve, 2000));
  expect(add(1, 1)).toBe(2);
```

rqx says: wait on the thing itself rather than on the clock, or fake the timer

## `skipped-test`

a skipped test is a question nobody answered; it reports green forever while the behaviour it named goes unchecked

| | |
|---|---|
| Measures | test declared with skip, xit, xdescribe or todo |
| Reported as | warning, verified |
| Reads | resolved scopes and symbols |
| Auto-fixable | `rqx fix` (deletion) |
| Fix | Remove Dead Code |

From `fixtures/typescript/suite.test.ts`, line 3:

```ts

describe.skip("pricing", () => {
  it("discounts", () => { expect(add(1, 2)).toBe(3); });
});

```

rqx says: fix it and re-enable it, or delete it and record the question it was asking

## `focused-test`

this silently disables every other test in the file, so the suite goes green while almost nothing ran

| | |
|---|---|
| Measures | test declared with only, fit or fdescribe |
| Reported as | error, verified |
| Reads | resolved scopes and symbols |
| Auto-fixable | `rqx fix` (deletion) |
| Fix | Remove Dead Code |
| Not in Python | pytest and unittest focus a run with `-k` on the command line; there is nothing in the source to find |
| Not in Go | `go test` focuses a run with `-run` on the command line; there is nothing in the source to find |
| Not in Java | JUnit has no focus annotation; a run is narrowed by the build tool, not by the test |
| Not in C++ | GoogleTest focuses a run with `--gtest_filter` on the command line; there is nothing in the source to find |

From `fixtures/typescript/suite.test.ts`, line 7:

```ts

it.only("ships", () => { expect(add(1, 1)).toBe(2); });

```

rqx says: remove the focus before this lands; it is almost never meant to be committed

## `assertionless-test`

it passes as long as nothing throws, so it reports success for behaviour it never actually checked

| | |
|---|---|
| Measures | test body containing no assertion call |
| Reported as | warning, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Introduce Assertion |

From `fixtures/typescript/suite.test.ts`, line 9:

```ts

it("checks nothing", () => { const value = add(1, 1); });

```

rqx says: assert the behaviour it names, or delete it so the suite stops claiming coverage

