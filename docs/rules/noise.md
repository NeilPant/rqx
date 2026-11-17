# Noise

[Back to the catalogue](README.md)

## `comment-per-line`

the code is being read aloud rather than explained, so the reader now has two things to keep in step and only one of them is checked

| | |
|---|---|
| Measures | a run of statements each carrying a comment of its own |
| Reported as | info, measured threshold |
| Reads | the syntax tree alone |
| Fix | Remove Dead Code / Rename Variable |
| Tuned by | `thresholds.narrated_run` |

From `fixtures/typescript/hygiene.ts`, line 107:

```ts
export function narrated(rows: number[]): number {
  // set the total
  let total = 0;
  // walk the rows
  for (const row of rows) total += row;
  // halve it
  total = total / 2;
  // give it back
  return total;
```

Suggested fix: let the names say it, and keep the one comment that gives a reason

## `explanation-longer-than-code`

a body that needs more prose than code is a body whose names are not carrying it, and the prose is what goes stale

| | |
|---|---|
| Measures | lines of comment above a function against statements inside it |
| Reported as | info, measured threshold |
| Reads | the syntax tree alone |
| Fix | Extract Method / Rename Method |
| Tuned by | `thresholds.explanation_ratio` |

From `fixtures/typescript/hygiene.ts`, line 117:

```ts

// Works out the score for the given user id, and gives back the score it
// worked out, having looked at the id that was handed in by whoever called
// it, which is the id of the user whose score is wanted here. The id is not
// checked, because whoever calls this has already checked it, and checking
// twice would be slower for no benefit anybody has ever measured. If the id
// is wrong the answer is wrong, which is the same as it has always been in
// every version of this that anybody has written since the beginning.
export function scoreOf(userId: number): number {
```

Suggested fix: put what the prose is explaining into the names, and keep the part that says why

## `docstring-restates-signature`

it is the signature written twice, so it costs a reader a second pass and goes wrong the first time the signature changes

| | |
|---|---|
| Measures | a doc comment whose every word is already in the name or the parameters |
| Reported as | info, measured threshold |
| Reads | the syntax tree alone |
| Fix | Remove Dead Code |

From `fixtures/typescript/hygiene.ts`, line 128:

```ts

/** Score of user id */
export function scoreOfUser(userId: number): number {
```

Suggested fix: say why it exists, or delete it

## `over-logging`

the body says more about what it is doing than it does, and the reader has to separate the work from the commentary on every line

| | |
|---|---|
| Measures | log calls against the statements of the body they are in |
| Reported as | info, measured threshold |
| Reads | the syntax tree alone |
| Fix | Remove Dead Code |
| Tuned by | `thresholds.log_percent` |

From `fixtures/typescript/conventions.ts`, line 32:

```ts

export function narrates(rows: number[]): number {
  const log = { info: (message: string) => message };
```

Suggested fix: log the decision and the failure; the steps between are the code

## `banner-comment`

a table of contents for a file short enough not to need one, and it goes stale the moment a section moves

| | |
|---|---|
| Measures | a comment that is a divider or a one-word heading |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Auto-fixable | `rqx fix` (deletion) |
| Fix | Remove Dead Code |

From `fixtures/typescript/hygiene.ts`, line 40:

```ts

// ---------------- helpers ----------------
// Here's the helper you asked for! 🎉
```

Suggested fix: delete it; if the file needs sections, it needs splitting

## `chat-residue`

it is the residue of a conversation that ended before the commit, and it tells the next reader nothing about the code it sits on

| | |
|---|---|
| Measures | a comment addressed to a person rather than about the code |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Auto-fixable | `rqx fix` (deletion) |
| Fix | Remove Dead Code |

From `fixtures/typescript/hygiene.ts`, line 41:

```ts
// ---------------- helpers ----------------
// Here's the helper you asked for! 🎉
// Sorry, this one is ugly.
```

Suggested fix: say what the code does that its name does not, or delete the comment

## `apology-comment`

somebody already knew this was wrong and left it, so the comment records the decision without the reason or the ticket that would let it be undone

| | |
|---|---|
| Measures | a comment apologising for the code under it |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Remove Dead Code |

From `fixtures/typescript/hygiene.ts`, line 42:

```ts
// Here's the helper you asked for! 🎉
// Sorry, this one is ugly.

```

Suggested fix: fix it, or record why it stays and what would let it go

## `emoji-in-source`

it survives grep, diffs and terminals badly, and it is decoration in a place where everything else has to earn its width

| | |
|---|---|
| Measures | an emoji in a comment outside a test |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Remove Dead Code |
| Default | off; name it under `rules.enabled` |

From `fixtures/typescript/hygiene.ts`, line 41:

```ts
// ---------------- helpers ----------------
// Here's the helper you asked for! 🎉
// Sorry, this one is ugly.
```

Suggested fix: write it in words

## `example-value`

a stand-in that reads as real is the kind of value that ships, and the first person to notice is whoever it was sent to

| | |
|---|---|
| Measures | a placeholder like example.com or YOUR_API_KEY outside a test |
| Reported as | warning, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Replace Magic Number with Constant |

From `fixtures/typescript/config.ts`, line 5:

```ts
      if (retries > 0) {
        return "https://api.example.com/v2/orders";
      }
```

Suggested fix: name it as configuration, or move it to a test where a stand-in belongs

