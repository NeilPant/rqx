#!/bin/sh
# Runs one job's `script:` from .gitlab-ci.yml, here, in a shell.
#
# `gitlab-runner exec` was removed in GitLab 16, and `run-single` polls a real GitLab
# instance for work — neither runs a pipeline from a checkout. This reads the job's
# commands out of the YAML and runs them in order, which is what `exec` did and what
# you want before pushing.
#
# It is not a runner: no image, no cache, no artifacts, no `rules:` evaluation. A job
# that passes here can still fail in CI for a reason the container would have shown.
#
#     tools/ci-local.sh                 # list the jobs
#     tools/ci-local.sh lint            # run one
#     tools/ci-local.sh --in ../other   # run it against another checkout

set -eu

file=".gitlab-ci.yml"
target="."
job=""

while [ $# -gt 0 ]; do
    case "$1" in
        --in) target="$2"; shift 2 ;;
        --file) file="$2"; shift 2 ;;
        -*) echo "unknown option: $1" >&2; exit 2 ;;
        *) job="$1"; shift ;;
    esac
done

[ -f "$file" ] || { echo "no $file here" >&2; exit 2; }

jobs() {
    # A job is a top-level key that is not a keyword and not a `.hidden` template.
    awk '
        /^[a-zA-Z][a-zA-Z0-9_:-]*:/ {
            name = $1
            sub(/:$/, "", name)
            if (name != "stages" && name != "default" && name != "variables" &&
                name != "include" && name != "workflow")
                print name
        }
    ' "$file"
}

if [ -z "$job" ]; then
    echo "jobs in $file:"
    jobs | sed 's/^/  /'
    exit 0
fi

jobs | grep -qx "$job" || { echo "no job named '$job' in $file" >&2; jobs | sed 's/^/  /' >&2; exit 2; }

# The job's `script:` block: every list item until the indentation returns to a key.
script=$(awk -v want="$job:" '
    $0 == want { inside = 1; next }
    inside && /^[a-zA-Z.]/ { inside = 0 }
    inside && /^  script:/ { collecting = 1; next }
    collecting && /^  [a-zA-Z]/ { collecting = 0 }
    collecting { print }
' "$file")

[ -n "$script" ] || { echo "job '$job' has no script" >&2; exit 2; }

# Each command goes in its own file and runs under `sh`. A YAML literal block is one
# command that happens to span lines, so a separator that is a newline would split it
# into pieces that fail on their own.
steps=$(mktemp -d)
trap 'rm -rf "$steps"' EXIT

printf '%s\n' "$script" | awk -v out="$steps" '
    function flush() {
        if (buffer != "") {
            printf "%s", buffer > sprintf("%s/%03d", out, ++count)
            close(sprintf("%s/%03d", out, count))
            buffer = ""
        }
    }
    /^    - \|$/ { flush(); literal = 1; next }
    literal && /^      / { line = $0; sub(/^      /, "", line); buffer = buffer line "\n"; next }
    literal { flush(); literal = 0 }
    /^    - / { flush(); line = $0; sub(/^    - /, "", line); buffer = line "\n"; flush(); next }
    END { flush() }
'

echo "-- $job -- in $(cd "$target" && pwd)"

for step in "$steps"/*; do
    [ -f "$step" ] || continue
    printf '\n$ %s' "$(cat "$step")"
    # Captured before any test reads it: `if ! cmd` leaves `$?` holding the result of
    # the negation rather than of the command, which reports every failure as a zero.
    (cd "$target" && sh -e "$step") && status=0 || status=$?
    if [ "$status" -ne 0 ]; then
        printf '\n\342\234\226 %s exited %s\n' "$job" "$status"
        exit "$status"
    fi
done

printf '\n\342\234\223 %s\n' "$job"
