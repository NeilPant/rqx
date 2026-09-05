# The interface

`rqx` with no arguments, or `rqx scan .`, opens a full-screen interface. There is
no second surface: what the interface shows is what the command line prints, read
from the same scan by the same functions.

## When it opens, and when it does not

It opens when every one of these holds: stdout is a terminal, `TERM` is set and is
not `dumb`, `CI` is unset, the window is at least 60x20, `--no-tui` was not given,
and no flag that shapes the output was — `-q`, `--stats`, `--timings`, `--format`,
`--reporter`, `--watch`, or any gate flag. Otherwise the plain report is printed,
unchanged.

A filter given on the command line opens the interface already filtered, so
`rqx scan . --rule empty-catch` lands on those findings rather than on all of them.

`rqx check` never goes interactive. It exists for its exit code.

## The tabs

| # | Tab | Shows |
|---|---|---|
| 1 | Overview | health, the severity and grade distributions, and what to open first |
| 2 | Findings | every finding with the offending code marked |
| 3 | Files | one row per file, worst grade first, with churn and blast radius |
| 4 | Complexity | every function it measured; `p` flips to the bottlenecks |
| 5 | Graph | the cycles as rings, then what imports what; `c` for cycles only |
| 6 | Health | which family the score is losing to; `s` flips to slop |
| 7 | Plan | the work ranked by what it buys against what it costs |
| 8 | Patterns | which design patterns are half applied, and where one would fit |
| 9 | Rules | the catalogue, searchable |

Nine, because the digits reach nine. Settings are on `,` and in the palette, which
is where a thing looked at once belongs.

## Keys

| Key | Does |
|---|---|
| `tab` / `shift-tab` | next and previous tab |
| `1`–`9` | jump to a tab |
| `ctrl-p` or `:` | the command palette |
| `r` | scan the whole project again |
| `w` | watch, and re-analyse on every save |
| `/` | search within the screen |
| `f` | filter by rule, family and path, in the spelling the flags take |
| `g` | group the findings |
| `s` | severity floor, or on Health, flip to slop |
| `a` then `y` | prove a file's fixes, then write them |
| `x` | waive the selected finding, with a reason |
| `enter` | open the finding in `$EDITOR`, or open a row's file |
| `,` | the resolved settings |
| `?` | the keys of the screen you are on |
| `esc` | step back |
| `q` | leave, and print what the session found |

## The command palette

`ctrl-p` opens it over whatever is on screen. It holds every tab, every command,
every file in the last scan and every rule in the catalogue, and narrows as you
type: `cplx` reaches Complexity, `recomm` reaches the recommendations. `enter`
runs the row, `esc` closes.

`recommend`, `suppressions`, `patterns`, `techniques` and `levels` have no screen
of their own and do not need one — the palette opens them in a reading pane,
rendered by the same functions the command line calls, with colour off.

## The mouse

The wheel scrolls the list under the pointer. A click selects a row; a click on the
row already selected opens it, which is a double-click without having to time one.
The tab bar is clickable, and so are the key hints along the bottom — a hint sends
the key it names, so nothing is wired twice.

**Capture takes drag-select away from the terminal.** While the interface is
running, dragging selects nothing, because the terminal is handing the drag to rqx
instead of to itself. Most emulators pass `shift`+drag through to their own
selection; if yours does not, `--no-mouse` turns capture off for the session and
everything else keeps working.

## Refreshing

`a`/`y` and `x` write to the file under the cursor, and `w` watches for anyone else
writing to any of them. Either way only the file that changed is re-analysed. The
cross-file stage — the import graph, the clone index, the churn rules — still runs
over the whole project, because a deleted import changes what the graph says about
files nobody touched. A refreshed report is the report a full scan would give, and
a test asserts it.

## Leaving

`q` restores the terminal and prints one line saying what the session found and the
command that would print the whole thing. Raw mode, the alternate screen and mouse
capture come off together on a normal exit, on an error, and on a panic.

`--motion off` stops the animation without losing anything the screen says. The
interface only wakes while something is moving; a still screen blocks on input and
costs nothing to leave open.
