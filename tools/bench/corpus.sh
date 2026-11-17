#!/bin/sh
# Fetches the real-project benchmark corpus that tools/bench/corpus.lock pins, into
# target/bench-corpus/<name>, each at the commit on its line of the lock file.
#
#   tools/bench/corpus.sh          fetch the missing projects and restore the rest
#
# Safe to run again. A project already at its pinned commit has any edits reverted
# (run.sh's warm-after-edit scenario changes one file and leaves it changed), so every
# run starts from the pinned code. A project at another commit is moved to the pinned
# one. Nothing outside target/bench-corpus is touched.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
lock="$root/tools/bench/corpus.lock"
dest="$root/target/bench-corpus"

usage() {
    echo "usage: tools/bench/corpus.sh" >&2
    exit 2
}

[ $# -eq 0 ] || usage
[ -f "$lock" ] || { echo "corpus.sh: no $lock" >&2; exit 2; }

fetch_one() {
    name=$1
    url=$2
    sha=$3
    dir="$dest/$name"
    short=$(printf '%.12s' "$sha")

    if [ -d "$dir/.git" ] && [ "$(cd "$dir" && git rev-parse --verify -q HEAD || true)" = "$sha" ]; then
        (cd "$dir" && git checkout -q -- .)
        echo "  $name: at $short"
        return
    fi

    mkdir -p "$dir"
    (
        cd "$dir"
        [ -d .git ] || { git init -q && git remote add origin "$url"; }
        git fetch -q --depth 1 origin "$sha" </dev/null
        git checkout -q -f FETCH_HEAD
    )
    [ "$(cd "$dir" && git rev-parse HEAD)" = "$sha" ] ||
        { echo "corpus.sh: $name is not at $short after fetching" >&2; exit 1; }
    echo "  $name: fetched $short"
}

mkdir -p "$dest"
echo "corpus in $dest:"
while read -r name url sha; do
    case $name in
        '' | \#*) continue ;;
    esac
    fetch_one "$name" "$url" "$sha"
done <"$lock"
