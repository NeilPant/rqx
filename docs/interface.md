# The interactive session

`rqx` with no arguments opens a session in the terminal you are already in. `rqx
scan` and every other subcommand print their report and never open it. The session
adds no analysis of its own: each command in it is the command line's command,
parsed by the same definition and printed by the same functions.

## When it opens, and when it does not

Bare `rqx` opens the session when stdout is a terminal, `TERM` is set and is not
`dumb`, no pipeline variable such as `CI` is set, the window is at least 50 columns
by 14 rows, and `--no-tui` was not given. Otherwise it runs `rqx scan` on the working
directory and exits with that scan's exit code, so a pipeline that runs `rqx` gets
the report and a failing exit when the project cannot be scanned.

`rqx check` never goes interactive. It exists for its exit code.

## What it looks like

The session uses no alternate screen and does not capture the mouse. A few rows at
the bottom of the window are redrawn; everything else is printed above them into the
terminal's own scrollback, so you scroll, search and select with the terminal's own
tools, and what the session printed is still there after you leave.

```
╭────────────────────────────────────────────────╮
│ ▄▀▀▄▄▀▀▄  rqx 0.2.0                            │
│ █◉▓▓▓▓◉█  ~/work/billing · 3,456 files         │
│ ▀▄▄▄▄▄▄▀  health 82/100 · fair · 31 findings   │
╰────────────────────────────────────────────────╯
 ✓ Scanned 3,456 files in 0.41s · cache 3,455/3,456
 ● error    dead-export     src/api/client.ts:42
   … 28 more · /findings to list them
> /plan --budget 20
 1. Extract InvoiceTotals   −6 findings  +4 health
```

The owl is drawn larger in a real terminal; this is the layout, not the pixels.

The session scans the working directory as it opens. While it runs, the owl glances
left and right beside the phase, the files done and the file being read. When the
scan lands, the header and the worst findings go into scrollback and the prompt
takes the bottom of the window, with the owl beside it. The plumage follows the
health score, and the owl blinks every few seconds. `--motion off` draws it still.

The status line at the bottom is right-aligned: health, grade, findings, `watching`
when it is on, and how many files came from the cache. The key hints on the left
give way when the window is too narrow for both.

## Commands

A line starting with `/` is a command. It is parsed by the same clap definition as
`rqx <command>`, so `/plan --budget 20` and `rqx plan --budget 20` take the same
flags and print the same bytes. They run against the scan the session already holds,
so there is no second scan.

| Command | Runs |
|---|---|
| `/findings` | `rqx scan`, with every filter and display flag it takes |
| `/plan` `/health` `/complexity` `/perf` `/graph` `/recommend` `/patterns` `/suppressions` | the command of the same name, on the held scan |
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
`--coverage`, is refused with a line saying so, because the session's scan was
taken without it. Run that command from a shell. Commands the session does not host
(`init`, `config`, `lsp` and the like) say which `rqx` command to run instead. A
mistake in a command prints one red line, and `/<command> --help` prints that
command's own help.

`/fix` scans and can write, so it runs off the input thread, and a write is followed
by a rescan. `/check --fix` is refused; use `/fix` or run `rqx check --fix` from a
shell.

## Watching, opening and waiving

`/watch on` holds the project in memory and polls the tree for saves. A save
re-analyses that file alone and runs the cross-file stage over everything, the same
way `rqx scan --watch` stays correct, and the session prints what moved:

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
the file for an editor rqx has no line syntax for. The session gives the terminal to
the editor and takes it back when the editor exits. With neither variable set it
prints `file:line`.

`/waive src/api/client.ts:42 unused-export` is `rqx waive`. Left without `--reason`,
the prompt asks for one: type it, press enter, and the comment is written and the
project scanned again. `esc` cancels. `--dry-run` prints the diff instead.

## The prompt

| Key | Does |
|---|---|
| `←` `→` `home` `end` | move in the line; `ctrl-a` and `ctrl-e` do the same |
| `backspace` `delete` | remove before and at the cursor |
| `ctrl-u` | delete everything before the cursor |
| `ctrl-w` | delete the word before the cursor |
| `↑` `↓` | walk the history, or the menu when it is open |
| `tab` | accept the highlighted suggestion |
| `esc` | close the menu |
| `enter` | accept the suggestion if it would change the line, otherwise run the line |
| `ctrl-c` | stop the running job; at the prompt, clear the line, and twice within a second, leave |
| `ctrl-d` | leave |

History is kept per user in the platform state directory (`$XDG_STATE_HOME/rqx` or
`~/.local/state/rqx`, and `%LOCALAPPDATA%\rqx` on Windows), never in the repository.

### Completion

Typing `/` opens a menu. It offers command names first, ranked by how tightly the
letters you typed sit together, so `cplx` reaches `/complexity`. After a command it
offers that command's flags, read from the same definition the command line parses
with. After a flag it offers the values the flag takes: the choices clap lists, rule
slugs for `--rule` and `--not-rule`, families for `--category`, and files from the
scan for `--focus`. `/explain` offers rule slugs.

## Leaving

`/exit`, `ctrl-d`, or `ctrl-c` twice. The session clears its own rows, gives the
terminal back and prints one line saying what it found and the command that prints
the whole report. Raw mode is the only thing it takes from the terminal, and it is
given back on a normal exit, on an error and on a panic.

`--no-mouse` is still accepted so existing invocations keep working. It does
nothing, because the session never captures the mouse.
