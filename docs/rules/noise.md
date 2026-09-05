# Noise

[Back to the catalogue](README.md)

## `banner-comment`

a table of contents for a file short enough not to need one, and it goes stale the moment a section moves

| | |
|---|---|
| Measures | a comment that is a divider or a one-word heading |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Auto-fixable | `rqx fix` (deletion) |
| Fix | Remove Dead Code |

From `fixtures/typescript/hygiene.ts`, line 36:

```ts

// ---------------- helpers ----------------
// Here's the helper you asked for! 🎉
```

rqx says: delete it; if the file needs sections, it needs splitting

## `chat-residue`

it is the residue of a conversation that ended before the commit, and it tells the next reader nothing about the code it sits on

| | |
|---|---|
| Measures | a comment addressed to a person rather than about the code |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Auto-fixable | `rqx fix` (deletion) |
| Fix | Remove Dead Code |

From `fixtures/typescript/hygiene.ts`, line 37:

```ts
// ---------------- helpers ----------------
// Here's the helper you asked for! 🎉
// Sorry, this one is ugly.
```

rqx says: say what the code does that its name does not, or delete the comment

## `apology-comment`

somebody already knew this was wrong and left it, so the comment records the decision without the reason or the ticket that would let it be undone

| | |
|---|---|
| Measures | a comment apologising for the code under it |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Remove Dead Code |

From `fixtures/typescript/hygiene.ts`, line 38:

```ts
// Here's the helper you asked for! 🎉
// Sorry, this one is ugly.

```

rqx says: fix it, or record why it stays and what would let it go

## `emoji-in-source`

it survives grep, diffs and terminals badly, and it is decoration in a place where everything else has to earn its width

| | |
|---|---|
| Measures | an emoji in a comment outside a test |
| Reported as | info, measured threshold |
| Reads | resolved scopes and symbols |
| Fix | Remove Dead Code |
| Default | off; name it under `[rules] enabled` |

From `fixtures/typescript/hygiene.ts`, line 37:

```ts
// ---------------- helpers ----------------
// Here's the helper you asked for! 🎉
// Sorry, this one is ugly.
```

rqx says: write it in words

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

rqx says: name it as configuration, or move it to a test where a stand-in belongs

