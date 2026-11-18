# Tests

[Back to the catalogue](README.md)

## `slow-test`

a suite that is slow enough to be skipped stops being run, and a test that is not run provides no protection

| | |
|---|---|
| Measures | a sleep or timeout inside a test body above the configured pause |
| Reported as | warning, verified |
| Reads | resolved scopes and symbols |
| Fix | Replace Algorithm / Invert Dependency |
| Tuned by | `thresholds.slow_test_ms` |

From `fixtures/typescript/suite.test.ts`, line 12:

```ts
it("waits on the clock", async () => {
  await new Promise((resolve) => setTimeout(resolve, 2000));
  expect(add(1, 1)).toBe(2);
```

Suggested fix: wait for the condition itself instead of a fixed time, or fake the timer

## `real-world-test`

it fails for reasons unrelated to the code under test, and it is slow enough that it tends to be skipped

| | |
|---|---|
| Measures | a test reaching a database, a network or a disk |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Fix | Invert Dependency |
| Default | off; name it under `rules.enabled` |

From `fixtures/typescript/suite.test.ts`, line 41:

```ts
it("reads the store", async () => {
  const rows = await db.query("rows");
  expect(rows).toBeDefined();
```

Suggested fix: pass the test a test double, and test the real dependency once

## `unlabelled-assertions`

a failing run prints two values, and the reader has to count lines to find which assertion they came from

| | |
|---|---|
| Measures | a run of checks in one test with nothing said about any of them |
| Reported as | info, measured threshold |
| Reads | the syntax tree alone |
| Fix | Extract Method |
| Default | off; name it under `rules.enabled` |

From `fixtures/typescript/suite.test.ts`, line 22:

```ts

it("asserts the same thing twice", () => {
  expect(add(1, 1)).toBe(2);
  expect(add(1, 1)).toBe(2);
});

```

Suggested fix: state what each check verifies, or split them into tests named for what they verify

## `self-asserting-test`

it holds the code to whatever the code already does, so it keeps passing through every change, including a change that breaks the behaviour

| | |
|---|---|
| Measures | a check with the same expression on both sides |
| Reported as | error, measured threshold |
| Reads | the syntax tree alone |
| Fix | Introduce Assertion |

From `fixtures/typescript/suite.test.ts`, line 56:

```ts
it("holds the code to itself", () => {
  expect(add(1, 1)).toBe(add(1, 1));
});
```

Suggested fix: write the expected value as a literal, instead of the call that produces it

## `unexplained-expected-value`

when it changes, nobody can tell whether the code or the number is wrong, so the test tends to be edited until it passes

| | |
|---|---|
| Measures | a number a check expects with nothing saying where it came from |
| Reported as | info, measured threshold |
| Reads | the syntax tree alone |
| Fix | Replace Magic Number with Constant |
| Default | off; name it under `rules.enabled` |

From `fixtures/typescript/suite.test.ts`, line 4:

```ts
describe.skip("pricing", () => {
  it("discounts", () => { expect(add(1, 2)).toBe(3); });
});
```

Suggested fix: name it, or work it out in the test from the inputs it came from

## `empty-test`

it passes on every run and checks nothing, while counting towards the test total

| | |
|---|---|
| Measures | a test with an empty body |
| Reported as | warning, verified |
| Reads | the syntax tree alone |
| Fix | Remove Dead Code / Introduce Assertion |

From `fixtures/typescript/suite.test.ts`, line 63:

```ts

it("is not written yet", () => {});

```

Suggested fix: write the check it was meant to contain, or delete it

## `eager-test`

it fails if any of the assertions fail and the run reports only that the test failed, so the test name does not identify the cause

| | |
|---|---|
| Measures | distinct units one test exercises |
| Reported as | info, measured threshold |
| Reads | the syntax tree alone |
| Fix | Extract Method |
| Default | off; name it under `rules.enabled` |

From `fixtures/typescript/suite.test.ts`, line 72:

```ts

it("drives five units", () => {
  const a = add(1, 1);
  const b = pricing([1]);
  const c = ship({ customer: { address: { city: "x" } } });
  const d = process([1]);
  const e = manyPaths("a", [1]);
  expect(a + b + Number(c.length) + d + e).toBeGreaterThan(0);
});

```

Suggested fix: make each test check one unit, so a failing run names what broke

## `string-shape-assertion`

it checks how the result prints and not what it is, so a change in spacing fails a test about behaviour

| | |
|---|---|
| Measures | a check against a serialised shape written out as a string |
| Reported as | info, measured threshold |
| Reads | the syntax tree alone |
| Fix | Introduce Assertion |

From `fixtures/typescript/suite.test.ts`, line 69:

```ts
  expect(String(one)).toBe("2");
  expect(JSON.stringify({ two })).toBe('{"two":4,"spare":"padding here"}');
});
```

Suggested fix: check the parts that matter, so spacing cannot fail the test

## `unused-fixture`

every case in the file pays for it and none uses it, which adds time to every run and leaves a reader wondering what it is for

| | |
|---|---|
| Measures | a value a setup hook prepares that no test reads |
| Reported as | info, verified |
| Reads | the syntax tree alone |
| Fix | Remove Dead Code |

From `fixtures/typescript/suite.test.ts`, line 84:

```ts
beforeEach(() => {
  spare = 1;
});
```

Suggested fix: delete it, or move it into the one case that wanted it

## `unchecked-stub`

it let the test run and checked nothing, so the call it replaces could stop happening and the test would still pass

| | |
|---|---|
| Measures | a test double that is never checked for the calls it received |
| Reported as | info, measured threshold |
| Reads | the syntax tree alone |
| Fix | Introduce Assertion |

From `fixtures/typescript/suite.test.ts`, line 88:

```ts
it("stands something in", () => {
  const client = jest.fn();
  expect(add(1, 1)).toBe(2);
```

Suggested fix: assert on the calls it received, or use the real implementation

## `branching-test`

a test that branches has paths of its own and nothing says which one ran, so a passing result no longer shows that the case was checked

| | |
|---|---|
| Measures | a decision inside a test body |
| Reported as | info, verified |
| Reads | the syntax tree alone |
| Fix | Extract Method / Decompose Conditional |
| Tuned by | `thresholds.test_decisions` |

From `fixtures/typescript/suite-checks.test.ts`, line 43:

```ts

it("decides three ways", () => {
  let total = add(0, 1);
  if (total > 1) {
    total = add(total, -1);
  } else {
    total = add(total, 1);
  }
  expect(total).toBe(2);
});

```

Suggested fix: write one test per path, so a passing run shows which case was checked

## `duplicate-assertion`

the second assertion cannot fail while the first passes, so it costs the reader a check they have already read

| | |
|---|---|
| Measures | the same assertion written twice in one test |
| Reported as | info, verified |
| Reads | the syntax tree alone |
| Fix | Remove Dead Code |

From `fixtures/typescript/suite.test.ts`, line 22:

```ts

it("asserts the same thing twice", () => {
  expect(add(1, 1)).toBe(2);
  expect(add(1, 1)).toBe(2);
});

```

Suggested fix: delete the repeat, or assert what the second one was meant to check

## `hand-rolled-throws`

when the call stops failing, the handler never runs and the test still passes, which is the outcome the test was written to catch

| | |
|---|---|
| Measures | a test asserting a failure with a handler of its own |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Fix | Introduce Assertion |
| Not in Go | failure is a returned value the test compares directly |

From `fixtures/typescript/suite.test.ts`, line 28:

```ts
it("catches the failure itself", () => {
  try {
    add(1, 1);
  } catch (error) {
    expect(error).toBeDefined();
  }
});
```

Suggested fix: use the framework's expected-failure assertion, so a call that stops failing fails the test

## `debug-output-in-test`

it was added to observe the test once, and it now prints on every run

| | |
|---|---|
| Measures | a write to the terminal from inside a test |
| Reported as | info, verified |
| Reads | the syntax tree alone |
| Fix | Remove Dead Code |

From `fixtures/typescript/suite.test.ts`, line 36:

```ts
it("prints while it runs", () => {
  console.log("here");
  expect(add(1, 1)).toBe(2);
```

Suggested fix: assert the value that was being inspected, or delete the line

## `unnamed-test`

a failing run reports the test name and nothing else, so the name is all the reader has before opening the file

| | |
|---|---|
| Measures | a test whose name says nothing about what it checks |
| Reported as | info, verified |
| Reads | the syntax tree alone |
| Fix | Rename Method |

From `fixtures/typescript/suite.test.ts`, line 45:

```ts

it("works", () => {
  expect(add(1, 1)).toBe(2);
});

```

Suggested fix: name the behaviour it holds to, so a failing run says what broke

## `skipped-test`

a skipped test reports success while the behaviour it names goes unchecked

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

Suggested fix: fix the test and enable it, or delete it and record what it was checking

## `focused-test`

this silently disables every other test in the file, so the suite passes while almost nothing runs

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

Suggested fix: remove the focus before merging; a focused test is rarely meant to be committed

## `assertionless-test`

it passes as long as nothing throws, so it reports success for behaviour it never checked

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

Suggested fix: assert the behaviour it names, or delete it so the suite does not report coverage it lacks

