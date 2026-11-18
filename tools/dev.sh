#!/bin/sh
# Developer commands for rqx: the gates CI runs, the release binary exercised against
# fixtures/, and the cache and benchmark checks, from one entry point.
#
#   tools/dev.sh check             the CI gates: fmt, clippy, tests, generated docs
#   tools/dev.sh smoke             build release, run every command against fixtures/
#   tools/dev.sh ci                smoke as CI runs it: CI=1, NO_COLOR=1, no terminal
#   tools/dev.sh session [PATH]    open the interface on PATH (default fixtures/typescript)
#   tools/dev.sh cache             time a cold scan, a warm one, and one after an edit
#   tools/dev.sh bench [ARGS]      run tools/bench/run.sh ARGS, with the release build last
#   tools/dev.sh try PATH [ARGS]   run rqx ARGS (default: scan --no-tui) inside PATH
#   tools/dev.sh install-test      run the installer tests, when this branch has them
#   tools/dev.sh help              this list
#
# Exit codes match ./rqx and tools/ci-local.sh: 0 passed, 1 a check failed, 2 a usage
# error or a missing tool. Nothing here is hardcoded to one machine: the build uses
# whatever cargo, linker and CARGO_TARGET_DIR the environment provides.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
target_dir=${CARGO_TARGET_DIR:-$root/target}
rqx=$target_dir/release/rqx
failures=0

usage() {
    cat <<'EOF'
usage: tools/dev.sh <command> [args]

  check             the CI gates: fmt, clippy, tests, generated docs
  smoke             build release, run every command against fixtures/
  ci                smoke as CI runs it: CI=1, NO_COLOR=1, no terminal
  session [PATH]    open the interface on PATH (default fixtures/typescript)
  cache             time a cold scan, a warm one, and one after an edit
  bench [ARGS]      run tools/bench/run.sh ARGS, with the release build last
  try PATH [ARGS]   run rqx ARGS (default: scan --no-tui) inside PATH
  install-test      run the installer tests, when this branch has them
  help              this list
EOF
}

usage_error() {
    echo "dev.sh: $*" >&2
    echo "run tools/dev.sh help for the commands" >&2
    exit 2
}

no_args() {
    name=$1
    shift
    [ $# -eq 0 ] || usage_error "$name takes no arguments"
}

need() {
    command -v "$1" >/dev/null 2>&1 || {
        echo "dev.sh: $1 is not installed; it is needed for $2" >&2
        exit 2
    }
}

build_release() {
    cargo build --quiet --release --manifest-path "$root/Cargo.toml" --bin rqx
}

check() {
    no_args check "$@"
    cd "$root"
    echo "== fmt"
    cargo fmt --all -- --check
    echo "== clippy"
    cargo clippy --workspace --all-targets -- -D warnings
    echo "== test"
    cargo test --workspace
    echo "== generated docs"
    cargo run --quiet --release -- docs
    git diff --exit-code docs/
    echo "== bench summary"
    need python3 "the bench summary test"
    python3 -I tools/bench/test_summary.py
    echo "check passed"
}

# Each reporter's output gets a validator. Every validator takes a file and exits
# non-zero when the file is not what the format says it is.
none() {
    :
}

json_lines() {
    python3 -I -c 'import sys, json
[json.loads(line) for line in sys.stdin if line.strip()]' <"$1"
}

json_document() {
    python3 -I -c 'import sys, json
json.load(sys.stdin)' <"$1"
}

junit_document() {
    python3 -I -c 'import sys, xml.dom.minidom
xml.dom.minidom.parse(sys.stdin)' <"$1"
}

github_annotations() {
    awk 'NF && substr($0, 1, 2) != "::" { bad = 1 } END { exit bad || NR == 0 }' "$1"
}

report() {
    printf '  %-5s %-34s %s\n' "$1" "$2" "$3"
    [ "$1" = PASS ] || failures=$((failures + 1))
}

results=""
case_number=0

# smoke_case NAME WANT_EXIT VALIDATOR ARGS...
# Runs rqx with ARGS and passes when it exits with WANT_EXIT, its output passes
# VALIDATOR, and no ANSI escape sequence appears in what it printed.
smoke_case() {
    name=$1
    want=$2
    validator=$3
    shift 3
    case_number=$((case_number + 1))
    stdout="$results/$case_number.out"
    stderr="$results/$case_number.err"

    got=0
    "$rqx" "$@" </dev/null >"$stdout" 2>"$stderr" || got=$?

    if [ "$got" -ne "$want" ]; then
        report FAIL "$name" "exit $got, wanted $want: $(head -n 1 "$stderr")"
    elif ! "$validator" "$stdout"; then
        report FAIL "$name" "output is not valid $validator"
    elif grep -qF "$(printf '\033')[" "$stdout" "$stderr"; then
        report FAIL "$name" "printed an escape sequence"
    else
        report PASS "$name" "exit $got"
    fi
}

run_smoke() {
    need python3 "the reporter output checks"
    cd "$root"
    build_release
    export RQX_NO_UPDATE_CHECK=1

    results=$(mktemp -d)
    trap 'rm -rf "$results"' EXIT

    rule=long-function
    "$rqx" rules --slugs --no-tui </dev/null >"$results/slugs"
    if ! grep -qx "$rule" "$results/slugs"; then
        echo "dev.sh: $rule is not a rule slug; see rqx rules --slugs" >&2
        exit 1
    fi

    echo "rqx smoke: $rqx"
    report_header
    smoke_case "scan" 0 none scan fixtures --no-tui
    smoke_case "scan --fail-on error" 1 none scan fixtures --no-tui --fail-on error
    smoke_case "scan --reporter json" 0 json_lines scan fixtures --no-tui --reporter json
    smoke_case "scan --reporter sarif" 0 json_document scan fixtures --no-tui --reporter sarif
    smoke_case "scan --reporter gitlab" 0 json_document scan fixtures --no-tui --reporter gitlab
    smoke_case "scan --reporter junit" 0 junit_document scan fixtures --no-tui --reporter junit
    smoke_case "scan --reporter github" 0 github_annotations scan fixtures --no-tui --reporter github
    smoke_case "complexity" 0 none complexity fixtures --no-tui
    smoke_case "perf" 0 none perf fixtures --no-tui
    smoke_case "health" 0 none health fixtures --no-tui
    smoke_case "slop" 0 none slop fixtures --no-tui
    smoke_case "graph" 0 none graph fixtures --no-tui
    smoke_case "plan" 0 none plan fixtures --no-tui
    smoke_case "patterns" 0 none patterns fixtures --no-tui
    smoke_case "recommend" 0 none recommend fixtures --no-tui
    smoke_case "rules" 0 none rules --no-tui
    smoke_case "explain $rule" 0 none explain "$rule" --no-tui
    smoke_case "suppressions" 0 none suppressions fixtures --no-tui
    smoke_case "config" 0 none config fixtures --no-tui
    smoke_case "techniques" 0 none techniques --no-tui
    smoke_case "levels" 0 none levels --no-tui

    echo
    if [ "$failures" -eq 0 ]; then
        echo "all smoke cases passed"
    else
        echo "$failures smoke case(s) failed" >&2
        exit 1
    fi
}

report_header() {
    printf '  %-5s %-34s %s\n' RESULT CASE DETAIL
}

smoke() {
    no_args smoke "$@"
    run_smoke
}

ci() {
    no_args ci "$@"
    CI=1
    NO_COLOR=1
    export CI NO_COLOR
    run_smoke
}

session() {
    [ $# -le 1 ] || usage_error "session takes one PATH"
    project=${1:-$root/fixtures/typescript}
    [ -d "$project" ] || usage_error "not a directory: $project"
    project=$(CDPATH= cd -- "$project" && pwd)
    exec "$root/rqx" scan "$project"
}

# Prints the wall time of one rqx run as seconds. /usr/bin/time -p writes its timing
# to stderr after the program's own output, so the `real` line is found by its label.
timed() {
    label=$1
    shift
    if [ -x /usr/bin/time ]; then
        status=0
        /usr/bin/time -p "$rqx" "$@" </dev/null >/dev/null 2>"$work/time.txt" || status=$?
        [ "$status" -eq 0 ] || { echo "dev.sh: rqx exited $status during $label" >&2; exit 1; }
        seconds=$(grep '^real' "$work/time.txt" | awk '{ print $2 }')
        printf '  %-18s %s s\n' "$label" "$seconds"
    else
        "$rqx" "$@" </dev/null >/dev/null
        printf '  %-18s not timed (no /usr/bin/time)\n' "$label"
    fi
}

cache() {
    no_args cache "$@"
    cd "$root"
    build_release
    export RQX_NO_UPDATE_CHECK=1

    work=$(mktemp -d)
    trap 'rm -rf "$work"' EXIT
    cp -R fixtures "$work/fixtures"
    project="$work/fixtures"

    "$rqx" cache clear "$project" </dev/null >/dev/null
    echo "wall time, $project:"
    timed "cold" scan "$project" --no-tui --timings
    timed "warm" scan "$project" --no-tui --timings

    edited=$(find "$project" -name '*.ts' | sort | head -n 1)
    [ -n "$edited" ] || { echo "dev.sh: no .ts file under fixtures/ to edit" >&2; exit 1; }
    printf '\n// touched by tools/dev.sh cache\n' >>"$edited"
    timed "warm after edit" scan "$project" --no-tui --timings
}

bench() {
    cd "$root"
    script="$root/tools/bench/run.sh"
    [ -f "$script" ] || { echo "dev.sh: tools/bench/run.sh is not on this branch" >&2; exit 2; }
    build_release
    exec sh "$script" "$@" "$rqx"
}

try() {
    [ $# -ge 1 ] || usage_error "try needs a PATH"
    [ -d "$1" ] || usage_error "not a directory: $1"
    project=$(CDPATH= cd -- "$1" && pwd)
    shift
    [ $# -gt 0 ] || set -- scan --no-tui
    cd "$root"
    build_release
    cd "$project"
    exec "$rqx" "$@"
}

install_test() {
    script="$root/tools/install-test.sh"
    if [ -f "$script" ]; then
        exec sh "$script" "$@"
    fi
    echo "installer tests not present on this branch"
}

command_name=${1:-help}
if [ $# -gt 0 ]; then
    shift
fi

case $command_name in
    check) check "$@" ;;
    smoke) smoke "$@" ;;
    ci) ci "$@" ;;
    session) session "$@" ;;
    cache) cache "$@" ;;
    bench) bench "$@" ;;
    try) try "$@" ;;
    install-test) install_test "$@" ;;
    help | -h | --help) usage ;;
    *) usage_error "unknown command: $command_name" ;;
esac
