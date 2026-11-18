#!/bin/sh
# Times `rqx scan` with hyperfine in three scenarios: cold (cache cleared before every
# run), warm (cache already filled), and warm-after-edit (cache filled, then one file
# changed before every run). Prints a summary and writes each scenario's raw numbers to
# target/bench/<scenario>.json.
#
#   tools/bench/run.sh [--baseline OLD_BIN] [--corpus synth|real] BIN
#
#   BIN         the rqx binary to measure
#   --baseline  a second binary, timed in the same hyperfine call as BIN, so both
#               numbers come from the same machine state
#   --corpus    synth (default): the copies from synth.sh
#               real: the projects corpus.sh fetched
#
# Needs hyperfine and python3. Neither is installed by this script.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
out="$root/target/bench"
baseline=""
corpus=synth
bin=""

usage() {
    echo "usage: tools/bench/run.sh [--baseline OLD_BIN] [--corpus synth|real] BIN" >&2
    exit 2
}

absolute_binary() {
    [ -f "$1" ] && [ -x "$1" ] || { echo "run.sh: not an executable file: $1" >&2; exit 2; }
    printf '%s/%s' "$(CDPATH= cd -- "$(dirname -- "$1")" && pwd)" "$(basename -- "$1")"
}

while [ $# -gt 0 ]; do
    case $1 in
        --baseline)
            [ $# -ge 2 ] || usage
            baseline=$2
            shift 2
            ;;
        --corpus)
            [ $# -ge 2 ] || usage
            corpus=$2
            shift 2
            ;;
        -*) usage ;;
        *)
            [ -z "$bin" ] || usage
            bin=$1
            shift
            ;;
    esac
done

[ -n "$bin" ] || usage
case $corpus in
    synth) corpus_dir="$root/target/bench-synth" ;;
    real) corpus_dir="$root/target/bench-corpus" ;;
    *) usage ;;
esac

command -v hyperfine >/dev/null 2>&1 || { echo "run.sh: hyperfine is not installed; it is needed to time runs" >&2; exit 2; }
command -v python3 >/dev/null 2>&1 || { echo "run.sh: python3 is not installed; it is needed for the summary" >&2; exit 2; }
if [ ! -d "$corpus_dir" ]; then
    setup=corpus.sh
    [ "$corpus" = synth ] && setup=synth.sh
    echo "run.sh: no $corpus corpus at $corpus_dir; run tools/bench/$setup first" >&2
    exit 2
fi

bin=$(absolute_binary "$bin")
[ -z "$baseline" ] || baseline=$(absolute_binary "$baseline")

export RQX_NO_UPDATE_CHECK=1
mkdir -p "$out"
cd "$corpus_dir"

# The edit goes to the first TypeScript file, which exists in every corpus. The
# comment is plain, so the file still parses and the change is only a new line.
edit_prepare='f=$(find . -name "*.ts" | head -n 1); printf "\n// bench edit\n" >> "$f"'

# prepare_for KIND BIN: the command hyperfine runs before each timed run of BIN.
prepare_for() {
    case $1 in
        clear) printf '"%s" cache clear . >/dev/null' "$2" ;;
        edit) printf '%s' "$edit_prepare" ;;
    esac
}

scan_command() {
    printf '"%s" scan --no-tui --reporter json > /dev/null' "$1"
}

# measure SCENARIO KIND: one hyperfine call. With a baseline, both binaries run in it,
# and each gets its own prepare, in the same order as the commands.
measure() {
    scenario=$1
    kind=$2
    # The commands redirect output and the edit prepare is a small shell script, so
    # hyperfine is told to hand them to the shell. It refuses to guess when a command
    # holds unquoted shell syntax, and a run that fails there measures nothing.
    set -- hyperfine --shell=default --warmup 1 --runs 10 --export-json "$out/$scenario.json"
    if [ "$kind" != none ]; then
        [ -z "$baseline" ] || set -- "$@" --prepare "$(prepare_for "$kind" "$baseline")"
        set -- "$@" --prepare "$(prepare_for "$kind" "$bin")"
    fi
    if [ -n "$baseline" ]; then
        set -- "$@" -n baseline "$(scan_command "$baseline")"
    fi
    set -- "$@" -n current "$(scan_command "$bin")"
    echo
    echo "== $scenario ($corpus)"
    "$@"
}

measure cold clear
measure warm none
measure warm-after-edit edit

echo
python3 -I "$root/tools/bench/summary.py" "$out" "${baseline:+yes}"
