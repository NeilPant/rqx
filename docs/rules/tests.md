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

## `real-world-test`

it fails for reasons that have nothing to do with the code, and it is slow enough that somebody eventually stops running it

| | |
|---|---|
| Measures | a test reaching a database, a network or a disk |
| Reported as | warning, measured threshold |
| Reads | the syntax tree alone |
| Fix | Invert Dependency |
| Default | off; name it under `[rules] enabled` |

From `fixtures/typescript/suite.test.ts`, line 41:

```ts
it("reads the store", async () => {
  const rows = await db.query("rows");
  expect(rows).toBeDefined();
```

rqx says: hand the test something standing in for it, and test the real one once

## `unlabelled-assertions`

a failing run prints two values and the reader has to count lines to find which check they came from

| | |
|---|---|
| Measures | a run of checks in one test with nothing said about any of them |
| Reported as | info, measured threshold |
| Reads | the syntax tree alone |
| Fix | Introduce Assertion |
| Default | off; name it under `[rules] enabled` |

From `fixtures/typescript/suite.test.ts`, line 49:

```ts

it("counts three ways", () => {
  expect(add(1, 1)).toBe(2);
  expect(add(2, 2)).toBe(4);
  expect(add(3, 3)).toBe(6);
});

```

rqx says: say what each one is holding to, or split them into tests that say it in their names

## `self-asserting-test`

it holds the code to whatever the code already does, so it passes through every change including the one that breaks it

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

rqx says: write the answer the test expects, not the call that produces it

## `unexplained-expected-value`

when it changes, nobody can tell whether the code is wrong or the number was, so the test is edited until it passes

| | |
|---|---|
| Measures | a number a check expects with nothing saying where it came from |
| Reported as | info, measured threshold |
| Reads | the syntax tree alone |
| Fix | Replace Magic Number with Constant |
| Default | off; name it under `[rules] enabled` |

From `fixtures/typescript/suite.test.ts`, line 4:

```ts
describe.skip("pricing", () => {
  it("discounts", () => { expect(add(1, 2)).toBe(3); });
});
```

rqx says: name it, or work it out in the test from the inputs it came from

## `branching-test`

a test that decides has paths of its own, and nothing says which one ran, so a green result no longer means the case was checked

| | |
|---|---|
| Measures | a decision inside a test body |
| Reported as | warning, verified |
| Reads | the syntax tree alone |
| Fix | Extract Method / Decompose Conditional |

From `fixtures/typescript/suite.test.ts`, line 16:

```ts

it("branches on the input", () => {
  if (add(1, 1) === 2) {
    expect(add(1, 1)).toBe(2);
  }
});

```

rqx says: write one test per path, so a green run says which case held

## `duplicate-assertion`

the second one can never fail while the first passes, so it costs a reader a check they have already read

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

rqx says: delete the repeat, or assert the thing the second one meant to

## `hand-rolled-throws`

when the call stops failing the handler never runs and the test still passes, which is the one outcome it was written to catch

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

rqx says: use whatever this framework spells as "expect it to fail", so a call that stops failing fails the test

## `debug-output-in-test`

it was how somebody watched this test once, and it now prints on every run for people who are not watching

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

rqx says: assert what you were reading, or delete the line

## `unnamed-test`

a failing run names the test and nothing else, so the name is the whole of what the reader is given before they open the file

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

rqx says: name the behaviour it holds to, so a failing run says what broke

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

