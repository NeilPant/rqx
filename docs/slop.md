# Code that reads as unread

Some code was written and never read back. Not badly designed — unread. A comment
above every line restating it, a divider comment in a fifty-line file, `example.com`
in a constant, an apology, a leftover *Here's the implementation you asked for!*

Each of those on its own is nothing. A style, a habit, a hurry. Together and at
density they describe a particular failure, and it needs a different response from
the rest of the catalogue: read the file, do not refactor it.

```sh
rqx slop
```

```
  Reads as unread  1 file · heuristic, and never a gate unless you ask

     96 generated.ts  5 statements
        chat-residue (10)  ·  apology-comment (6)  ·  example-value (5)  ·  banner-comment (4)  ·  emoji-in-source (4)

    the number is a suspicion, not a verdict — read the file before deciding what it needs
```

The signals are listed with their weights so the number can be checked rather than
taken on trust.

## The `noise` family

| rule | |
|---|---|
| `chat-residue` | a comment addressed to a person — *Here's*, *Let me*, *as requested*, *Hope this helps* |
| `banner-comment` | a divider or one-word heading: `// ---- helpers ----` |
| `apology-comment` | *sorry*, *this is a hack*, *I know this is bad* |
| `example-value` | `example.com`, `John Doe`, `YOUR_API_KEY` outside a test |
| `emoji-in-source` | an emoji in a comment; off by default |

`chat-residue` and `banner-comment` are auto-fixable — `rqx fix` deletes them, the
same edit it already makes for `journal-comment`.

These are `Info` apart from `example-value`, which is a `Warning` because a
placeholder that reads as real is the kind of value that ships.

## The score

Weighted signals per hundred statements, on a curve. Around one weighted signal per
fifteen statements lands near 60.

Two things stop it crying wolf. A file is scored as though it were at least 150
statements long, because a four-line file with a banner comment has one signal per
line and means nothing by it. And a total under 8 scores zero — one lapse is a lapse.
The lightest single signal worth a score is a comment written to a person.

It is a heuristic and says so everywhere it appears. `[gate] max_slop` exists if you
want it to fail a build, and no level sets it.

## What it is not

Not a plagiarism detector, not a *was this AI-written* oracle, and not an accusation.
Plenty of good code has a banner comment; plenty of AI-written code has none of these.
What the score measures is how much text in a file costs a reader and buys nothing —
which is worth knowing whoever typed it.
