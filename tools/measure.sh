#!/bin/sh
# Compares how many findings each rule reports under two rqx binaries on one path, so a
# change to a rule can be shown to move what it was meant to move and nothing else.
#
#   tools/measure.sh BASE_BINARY NEW_BINARY PATH [SCAN_ARGS...]
#
# BASE_BINARY is a build without the change (a release, or a build of main) and
# NEW_BINARY is one with it. Both scan PATH with the cache off and the update check off,
# and nothing is written under PATH: the script reports it if a file or directory there
# changed during the runs. Extra arguments go to `rqx scan`, for example
# `--language go` or a `--config` file.
#
# Prints one row per rule whose count differs, then the totals. Exit codes: 0 compared,
# 2 a usage error or a missing tool.
set -eu

usage() {
    echo "usage: tools/measure.sh BASE_BINARY NEW_BINARY PATH [SCAN_ARGS...]" >&2
    exit 2
}

[ $# -ge 3 ] || usage
base=$1
new=$2
target=$3
shift 3

for binary in "$base" "$new"; do
    [ -x "$binary" ] || { echo "measure.sh: $binary is not an executable file" >&2; exit 2; }
done
[ -d "$target" ] || { echo "measure.sh: $target is not a directory" >&2; exit 2; }
command -v python3 >/dev/null 2>&1 || { echo "measure.sh: python3 is needed" >&2; exit 2; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
touch "$work/started"

# A binary that fails its scan has nothing to compare, so the script stops there instead
# of reporting every rule as gone. Exit 1 only means the gate failed, which is not an
# error here.
run() {
    binary=$1
    output=$2
    shift 2
    status=0
    RQX_NO_CACHE=1 RQX_NO_UPDATE_CHECK=1 "$binary" scan "$target" --reporter json \
        --max-findings 0 --no-tui "$@" </dev/null >"$output" 2>"$output.err" || status=$?
    [ "$status" -le 1 ] || {
        echo "measure.sh: $binary exited $status" >&2
        cat "$output.err" >&2
        exit 2
    }
}

run "$base" "$work/base.ndjson" "$@"
run "$new" "$work/new.ndjson" "$@"

touched=$(find "$target" -newer "$work/started" -not -path '*/.git/*' 2>/dev/null | head -n 5)
if [ -n "$touched" ]; then
    echo "measure.sh: something under $target changed while scanning:" >&2
    echo "$touched" >&2
fi

python3 -I - "$work/base.ndjson" "$work/new.ndjson" <<'PYTHON'
import collections
import json
import sys


def counts(path):
    tally = collections.Counter()
    with open(path, encoding="utf-8") as lines:
        for line in lines:
            line = line.strip()
            if not line:
                continue
            record = json.loads(line)
            if record.get("kind") == "finding":
                tally[record["rule"]] += 1
    return tally


base, new = counts(sys.argv[1]), counts(sys.argv[2])
rows = [
    (rule, base[rule], new[rule])
    for rule in sorted(set(base) | set(new))
    if base[rule] != new[rule]
]
if rows:
    width = max(len(rule) for rule, _, _ in rows)
    print(f"{'rule':<{width}}  {'base':>6}  {'new':>6}  {'change':>7}")
    for rule, before, after in rows:
        print(f"{rule:<{width}}  {before:>6}  {after:>6}  {after - before:>+7}")
    print()
else:
    print("no rule reports a different number of findings")
print(f"total: {sum(base.values())} -> {sum(new.values())} findings; rules with a different count: {len(rows)}")
PYTHON
