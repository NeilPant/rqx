#!/bin/sh
# Checks install.sh on this machine without installing anything.
#
#     tools/install-test.sh
#
# Three checks run the real installer: a dry run must name the release tarball for this
# platform, a download that fails must leave the install directory empty, and --help
# must exit 0. When shellcheck is installed, it also lints install.sh and this file.
#
# The dry run asks GitHub for the latest release, so it needs the network. Without it,
# the installer says it could not reach GitHub and the check is skipped, not failed.
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

# The same mapping install.sh uses, so the test expects the platform the installer should pick.
expected_platform() {
  case "$(uname -m)" in
    x86_64 | amd64) arch="x64" ;;
    arm64 | aarch64) arch="arm64" ;;
    *) return 1 ;;
  esac
  case "$(uname -s)" in
    Darwin)
      if [ "$arch" = "x64" ] && [ "$(sysctl -n hw.optional.arm64 2>/dev/null || true)" = "1" ]; then
        arch="arm64"
      fi
      echo "darwin-$arch"
      ;;
    Linux) echo "linux-$arch" ;;
    *) return 1 ;;
  esac
}

check_dry_run() {
  name="dry run names this platform's tarball"
  platform="$(expected_platform)" || platform=""
  if [ -z "$platform" ]; then
    fail "$name: no expected platform for this platform"
    return
  fi

  status=0
  output="$(sh "$installer" --dry-run 2>&1)" || status=$?
  if [ "$status" -eq 0 ]; then
    case "$output" in
      *"-$platform.tar.gz"*)
        pass "$name ($platform)"
        return
        ;;
    esac
  fi

  case "$output" in
    *"could not reach"*"/releases/latest"*)
      echo "SKIP  $name: GitHub is not reachable from here"
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
