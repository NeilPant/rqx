#!/usr/bin/env bash
# The label set the issue templates assume. Idempotent: run it again after adding a
# family or a language and only the new ones appear.
set -euo pipefail

label() {
  gh label create "$1" --color "$2" --description "$3" --force >/dev/null
  printf '  %s\n' "$1"
}

echo "labels:"
label "bug"              "e4574a" "Something crashed, hung, or answered wrongly"
label "false-positive"   "e8a83a" "A rule fired on code that is fine"
label "rule"             "c89b6a" "A rule to add, change or retire"
label "performance"      "8fb55c" "Speed or memory"
label "docs"             "9e8f7c" "Documentation only"
label "good first issue" "8fb55c" "Small, self-contained, well specified"
label "help wanted"      "c89b6a" "Would welcome a pull request"

for family in size abstraction change waste coupling flow naming safety tests noise; do
  label "family:$family" "6e6150" "The $family rule family"
done

for language in typescript javascript python go java cpp; do
  label "language:$language" "785d3f" "Specific to $language"
done

for surface in editor tui mcp lsp ci config; do
  label "area:$surface" "9e8f7c" "The $surface surface"
done
