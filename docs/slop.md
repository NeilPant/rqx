# Code that reads as unread

`rqx slop` scores files that show signs of not having been reviewed after they were
written. The signals are a comment above every line that restates it, a divider
comment in a fifty-line file, `example.com` in a constant, an apology, and a leftover
*Here's the implementation you asked for!*.

One such signal is not significant. A high density of them indicates a file that
needs to be read, not refactored.

```sh
rqx slop
```

```
  Reads as unread  1 file · heuristic, and never a gate unless you ask

     96 generated.ts  5 statements
        chat-residue (10)  ·  apology-comment (6)  ·  example-value (5)  ·  banner-comment (4)  ·  emoji-in-source (4)

    the number is a suspicion, not a verdict — read the file before deciding what it needs
```

The signals are listed with their weights, so the score can be checked.

## The `noise` family

| rule | |
|---|---|
| `chat-residue` | a comment addressed to a person, such as *Here's*, *Let me*, *as requested*, *Hope this helps* |
| `banner-comment` | a divider or one-word heading: `// ---- helpers ----` |
| `apology-comment` | *sorry*, *this is a hack*, *I know this is bad* |
| `example-value` | `example.com`, `John Doe`, `YOUR_API_KEY` outside a test |
| `emoji-in-source` | an emoji in a comment; off by default |

`chat-residue` and `banner-comment` are auto-fixable: `rqx fix` deletes them, as it
does for `journal-comment`.

The rules are `Info`, except `example-value`, which is a `Warning` because a
placeholder value can be shipped as if it were real.

## The score

The score is the weighted signals per hundred statements, mapped through a curve.
About one weighted signal per fifteen statements gives a score near 60.

Two rules limit the score for small inputs. A file is scored as though it had at least
150 statements, because a short file with one banner comment has a high signal
density that carries little information. A total weight under 8 scores zero. The
lightest single signal that produces a score is a comment written to a person.

The score is a heuristic, and the output says so. The `gate` table has no key for the
slop score. `rqx slop --min` hides files that score below the given value.

## What it is not

The score does not detect plagiarism and does not determine whether code was written
by an AI tool. Code written by a person can contain a banner comment, and code written
by an AI tool can contain none of these signals. The score measures how much text in
a file adds reading cost without adding information, whoever wrote it.
