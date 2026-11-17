#!/bin/sh
# Makes N copies of fixtures/ under target/bench-synth/<i>/, the synthetic benchmark
# corpus. It needs no network, so it runs anywhere; the real projects in corpus.lock
# are the better measure when the network is available.
#
#   tools/bench/synth.sh [N]       N copies, default 20
#
# Safe to run again: each copy is written over the top, which also restores any file
# that run.sh's warm-after-edit scenario changed.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
dest="$root/target/bench-synth"
count=${1:-20}

usage() {
    echo "usage: tools/bench/synth.sh [N]   (N is a positive whole number, default 20)" >&2
    exit 2
}

case $count in
    '' | *[!0-9]*) usage ;;
esac
[ "$count" -ge 1 ] || usage
[ $# -le 1 ] || usage

i=1
while [ "$i" -le "$count" ]; do
    mkdir -p "$dest/$i"
    cp -R "$root/fixtures/." "$dest/$i/"
    i=$((i + 1))
done

echo "wrote $count copies of fixtures/ to $dest"
