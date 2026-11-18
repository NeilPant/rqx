# The interactive session

`rqx` with no arguments opens a session in the current terminal. `rqx scan` and
every other subcommand print their report and never open it. The session adds no
analysis of its own: each command in it is a command-line command, parsed by the
same definition and printed by the same functions.

## When it opens, and when it does not

Bare `rqx` opens the session when stdout is a terminal, `TERM` is set and is not
`dumb`, no pipeline variable such as `CI` is set, the window is at least 50 columns
by 14 rows, and `--no-tui` was not given. Otherwise it runs `rqx scan` on the working
directory and exits with that scan's exit code, so a pipeline that runs `rqx` gets
the report and a failing exit when the project cannot be scanned.

`rqx check` never opens the session. It is run for its exit code.

## What it looks like

The session uses no alternate screen and does not capture the mouse. A few rows at
the bottom of the window are redrawn. Everything else is printed above them into the
terminal's own scrollback, so scrolling, searching and selecting use the terminal's
own tools, and the output remains after the session ends.

```
╭────────────────────────────────────────────────╮
│ ▄█▄▄▄▄█▄  rqx 0.2.0                            │
│ █▀██▀▀█▀  ~/work/billing · 3,456 files         │
│ ▀██▀▀██▀  health 82/100 · fair · 31 findings   │
╰────────────────────────────────────────────────╯
 ✓ Scanned 3,456 files in 0.41s · cache 3,455/3,456
 1 ● error    dead-export     src/api/client.ts:42
   … 28 more · /findings to list them
────────────────────────────────────────────────────
> /plan --budget 20
────────────────────────────────────────────────────
 ? for shortcuts                    health 82 · fair · 31 findings · watching
```

The owl is three rows of half blocks, eight columns wide. The sketch above shows the
layout only.

The session scans the working directory as it opens. While the scan runs, the owl
sits beside the phase, the number of files done and the file being read, and reads:
its pupils sweep along a line and jump back to the start of the next, and every few
seconds it looks up. It does the same while any other command runs.

When the scan finishes, the header and the worst findings appear above the prompt,
which takes the full width at the bottom of the window. They stay in the redrawn rows
until the session prints anything else, such as the echo of a command or the `?`
list, and then move into scrollback unchanged. The header in scrollback shows the
owl's neutral face. While the header is still in the redrawn rows, the owl in it
moves:

| When | What the owl does |
|------|-------------------|
| The header appears | Wakes up over about 1.4 s, then reacts to the score for 0.8 s and keeps the face it ends on. |
| Health 80 or more, or no score | Blinks, then closes its eyes in a smile. It rests on a content face. |
| Health 50 to 79 | Looks down, half closes its eyes and blinks. It rests on the neutral face. |
| Health below 50 | Flares its ear tufts, then lowers its brow and eyes. It rests on a worried face. |
| At rest | Makes one small move every 4 to 10 seconds, chosen at random: a blink, a glance, a look down, a tuft twitch or a breath. |
| A key is pressed | Returns to its resting face. The next move waits at least 4 seconds. |

The owl's colouring follows the health score. When the window is too short to hold
the header, the findings and the prompt (with its command list when that is open),
the header and the findings go straight into scrollback and the owl in them does not
move.

`--motion off` draws the owl still everywhere: the neutral face in the colour of the
score. The header and the findings go into scrollback as soon as the scan finishes.

The line under the prompt shows `? for shortcuts` on the left and, on the right, the
health, the grade, the findings and `watching` when it is on. When the window is too
narrow for both, the shortcuts pointer is dropped. Nothing else is drawn. Everything
a command prints goes into scrollback.

## Commands

A line starting with `/` is a command. It is parsed by the same clap definition as
`rqx <command>`, so `/plan --budget 20` and `rqx plan --budget 20` take the same
flags and print the same bytes. They run against the scan the session already holds,
without scanning again.

| Command | Runs |
|---|---|
| `/findings` | `rqx scan`, with every filter and display flag it takes |
| `/hotspots` | `rqx hotspots`, which scans again with the churn window and the findings cap it needs |
| `/plan` `/health` `/owners` `/trend` `/complexity` `/perf` `/graph` `/recommend` `/patterns` `/suppressions` | the command of the same name, on the held scan |
| `/waive` | `rqx waive`; without `--reason` the prompt asks for one |
| `/explain` `/rules` `/techniques` `/levels` | the catalogue commands |
| `/slop` | `rqx slop`, which scans again with its own rules switched on |
| `/fix` `/check` `/verify` | the command of the same name; they scan or write, so they run in the background |
| `/rescan` | scan the project again |
| `/watch on\|off` | re-analyse files as they are saved |
| `/open` | open a finding in `$VISUAL` or `$EDITOR` |
| `/clear` `/help` `/exit` | clear the screen, list the commands, leave |

Text typed without a slash searches the findings: `dead export` is `/findings
--search "dead export"`.

A flag that would change the scan itself, such as a path, `--level`, `--config` or
`--coverage`, is refused with a message, because the session's scan was taken
without it. Such a command has to be run from a shell. Commands the session does
not host (`init`, `config`, `lsp` and similar) print the `rqx` command to run
instead. A mistake in a command prints one red line, and `/<command> --help` prints
that command's own help.

`/fix` scans and can write, so it runs off the input thread, and a write is followed
by a rescan. `/check --fix` is refused; use `/fix` or run `rqx check --fix` from a
shell.

## Watching, opening and waiving

`/watch on` holds the project in memory and polls the tree for saves. A save
re-analyses that file alone and runs the cross-file stage over everything, as
`rqx scan --watch` does, and the session prints what changed:

```
 src/api/client.ts  −1 finding · health 82 → 83
```

Up to three files are named on a line each, with the health on its own line; more
than that is counted. A save that changes no finding prints nothing. The status
line says `watching` while it is on, and the commands read the refreshed project, so
`/plan` after a fix is the plan for what is there now. `/watch off` stops it.

`/open 2` opens the second finding of the list under the header, and `/open
src/api/client.ts:42` opens that place. It runs `$VISUAL`, or `$EDITOR` when that
is empty, with the line: `vim +42 file`, `code -g file:42`, `hx file:42`, or just
the file for an editor rqx has no line syntax for. The session hands the terminal to
the editor and takes it back when the editor exits. With neither variable set, it
prints `file:line`.

`/waive src/api/client.ts:42 unused-export` runs `rqx waive`. Without `--reason`, the
prompt asks for one. After the reason is typed and `enter` is pressed, the comment
is written and the project is scanned again. `esc` cancels. `--dry-run` prints the
diff instead.

## The prompt

| Key | Does |
|---|---|
| `←` `→` `home` `end` | move in the line; `ctrl-a` and `ctrl-e` do the same |
| `backspace` `delete` | remove before and at the cursor |
| `ctrl-u` | delete everything before the cursor |
| `ctrl-w` | delete the word before the cursor |
| `↑` `↓` | walk the history, or the menu when it is open |
| `?` on an empty line | print the keys into scrollback |
| `tab` | accept the highlighted row; with no list open, list the command's flags |
| `esc` | close the list; it stays closed until a `-` is typed or `tab` is pressed |
| `enter` | accept the highlighted command if it would change the line, otherwise run the line |
| `ctrl-c` | stop the running job; at the prompt, clear the line, and twice within a second, leave |
| `ctrl-d` | leave |

History is kept per user in the platform state directory (`$XDG_STATE_HOME/rqx` or
`~/.local/state/rqx`, and `%LOCALAPPDATA%\rqx` on Windows), outside the repository.

### Completion

A line that starts with `/` shows a list of commands under the prompt. Each row has
the command and the first sentence of `rqx <command> --help`, and `tab` or `enter`
takes the highlighted row. The list is ranked by how closely the typed letters sit
together, so `cplx` matches `/complexity`. A line that does not start with `/`
shows no list.

After a command and a space the list stays closed, so a finished line runs on
`enter`. Typing `-` lists that command's flags, read from the same definition the
command line parses with, and `tab` lists them after a space. The flags that
`rqx <command> -h` shows come first and the rest follow, ranked by the typed
letters. A flag already on the line is not listed again unless it can be repeated.
After a flag the list offers the values the flag takes: the choices clap lists,
rule slugs for `--rule` and `--not-rule`, families for `--category`, and files from
the scan for `--focus`. `/explain` offers rule slugs.

## Leaving

`/exit`, `ctrl-d`, or `ctrl-c` twice. The session clears its own rows, restores the
terminal and prints one line with what it found and the command that prints the
whole report. Raw mode is the only terminal state the session changes. It is
restored on a normal exit, on an error and on a panic.

`--no-mouse` is still accepted so that existing invocations keep working. It has
no effect, because the session does not capture the mouse.
