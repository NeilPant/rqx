#!/bin/sh
# Checks install.sh on this machine without installing anything.
#
#     tools/install-test.sh
#
# Three checks run the real installer: a dry run must name the release tarball for this
# platform, a download that fails must leave the install directory empty, and --help
# must exit 0. When shellcheck is installed, it also lints install.sh and this file.
#
# The dry run asks GitHub for the latest release. While no release is published, the
# installer stops with a message saying so, and that counts as a pass; the output shows
# which message it was.
#
# Each check prints PASS, FAIL or SKIP. Any failure makes the script exit 1.

set -eu

root="$(cd "$(dirname "$0")/.." && pwd)"
installer="$root/install.sh"
script="$root/tools/install-test.sh"
failures=0
scratch=""

cleanup() {
  if [ -n "$scratch" ] && [ -d "$scratch" ]; then
    rm -rf "$scratch"
  fi
}
trap cleanup EXIT

pass() {
  echo "PASS  $1"
}

fail() {
  echo "FAIL  $1"
  failures=$((failures + 1))
}

# The same mapping install.sh uses, so the test expects the triple the installer should pick.
expected_triple() {
  case "$(uname -m)" in
    x86_64 | amd64) arch="x86_64" ;;
    arm64 | aarch64) arch="aarch64" ;;
    *) return 1 ;;
  esac
  case "$(uname -s)" in
    Darwin)
      if [ "$arch" = "x86_64" ] && [ "$(sysctl -n hw.optional.arm64 2>/dev/null || true)" = "1" ]; then
        arch="aarch64"
      fi
      echo "$arch-apple-darwin"
      ;;
    Linux) echo "$arch-unknown-linux-musl" ;;
    *) return 1 ;;
  esac
}

check_dry_run() {
  name="dry run names this platform's tarball"
  triple="$(expected_triple)" || triple=""
  if [ -z "$triple" ]; then
    fail "$name: no expected triple for this platform"
    return
  fi

  status=0
  output="$(sh "$installer" --dry-run 2>&1)" || status=$?
  if [ "$status" -eq 0 ]; then
    case "$output" in
      *"-$triple.tar.gz"*)
        pass "$name ($triple)"
        return
        ;;
    esac
  fi

  case "$output" in
    *"has no published release"* | *"could not reach"*"/releases/latest"*)
      pass "$name: no release is published yet, and the installer said so"
      ;;
    *)
      fail "$name"
      printf '%s\n' "$output" | sed 's/^/      /'
      ;;
  esac
}

check_failed_install_leaves_directory_empty() {
  name="a failed download installs nothing"
  scratch="$(mktemp -d "${TMPDIR:-/tmp}/rqx-install-test.XXXXXX")"

  status=0
  RQX_VERSION="0.0.0-nonexistent" RQX_INSTALL_DIR="$scratch" sh "$installer" >/dev/null 2>&1 || status=$?
  if [ "$status" -eq 0 ]; then
    fail "$name: the install succeeded for a version that does not exist"
  elif [ -n "$(ls -A "$scratch")" ]; then
    fail "$name: files were left in the install directory"
  else
    pass "$name"
  fi

  rm -rf "$scratch"
  scratch=""
}

check_help_exits_zero() {
  name="--help exits 0"
  if sh "$installer" --help >/dev/null 2>&1; then
    pass "$name"
  else
    fail "$name"
  fi
}

check_shellcheck() {
  name="shellcheck passes on install.sh and tools/install-test.sh"
  if ! command -v shellcheck >/dev/null 2>&1; then
    echo "SKIP  shellcheck is not installed, so the lint check was skipped"
    return
  fi
  if shellcheck "$installer" "$script"; then
    pass "$name"
  else
    fail "$name"
  fi
}

check_dry_run
check_failed_install_leaves_directory_empty
check_help_exits_zero
check_shellcheck

if [ "$failures" -eq 0 ]; then
  echo "All checks passed."
else
  echo "$failures check(s) failed."
  exit 1
fi
