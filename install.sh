#!/bin/sh
# Install the rqx binary for this machine from its GitHub release.
#
#   curl -fsSL https://raw.githubusercontent.com/jadhamwi21/rqx/main/install.sh | sh
#   sh install.sh [--version X.Y.Z] [--dir DIR] [--dry-run] [--help]
#
# What it does: works out the release's platform name from `uname`, downloads the
# tarball and SHA256SUMS over HTTPS into a throwaway directory, checks the tarball
# against SHA256SUMS, and only then copies the binary into the install directory.
# Any mismatch, missing checksum or missing hash tool stops it with nothing installed.
#
# What it never does: use sudo, edit a shell rc file, or change PATH. When the install
# directory is not on PATH it prints the line you could add and leaves the choice to you.

set -eu

REPOSITORY="jadhamwi21/rqx"
RELEASES_URL="https://github.com/$REPOSITORY/releases"
CARGO_FALLBACK="cargo install --git https://github.com/$REPOSITORY --locked rqx"

version="${RQX_VERSION:-}"
install_dir="${RQX_INSTALL_DIR:-${HOME:-}/.local/bin}"
dry_run=""
work_dir=""

usage() {
  cat <<'EOF'
Install rqx from its GitHub release.

Usage:
  curl -fsSL https://raw.githubusercontent.com/jadhamwi21/rqx/main/install.sh | sh
  sh install.sh [--version X.Y.Z] [--dir DIR] [--dry-run] [--help]

Options:
  --version X.Y.Z  Install this release instead of the latest (env: RQX_VERSION).
                   A prerelease such as 0.3.0-rc.1 is installed only when named here.
  --dir DIR        Install into DIR (env: RQX_INSTALL_DIR). Default: $HOME/.local/bin
  --dry-run        Print every step and the URLs involved, and change nothing.
  --help           Show this help.

The download is checked against the release's SHA256SUMS before anything is
installed. No sudo, and no changes to your shell configuration or PATH.
EOF
}

die() {
  echo "error: $*" >&2
  exit 1
}

die_with_fallback() {
  echo "error: $*" >&2
  echo "To build from source instead (needs the Rust toolchain):" >&2
  echo "  $CARGO_FALLBACK" >&2
  exit 1
}

step() {
  echo "  $*"
}

cleanup() {
  if [ -n "$work_dir" ] && [ -d "$work_dir" ]; then
    rm -rf "$work_dir"
  fi
}

have() {
  command -v "$1" >/dev/null 2>&1
}

parse_arguments() {
  while [ $# -gt 0 ]; do
    case "$1" in
      --version)
        [ $# -ge 2 ] || die "--version needs a value"
        version="$2"
        shift 2
        ;;
      --dir)
        [ $# -ge 2 ] || die "--dir needs a value"
        install_dir="$2"
        shift 2
        ;;
      --dry-run)
        dry_run="yes"
        shift
        ;;
      -h | --help)
        usage
        exit 0
        ;;
      *)
        usage >&2
        die "unknown option: $1"
        ;;
    esac
  done
}

detect_target() {
  os="$(uname -s)"
  machine="$(uname -m)"

  case "$machine" in
    x86_64 | amd64) arch="x64" ;;
    arm64 | aarch64) arch="arm64" ;;
    *) die_with_fallback "no rqx binary is published for the $machine architecture" ;;
  esac

  case "$os" in
    Darwin)
      # A shell started under Rosetta reports x86_64 on an Apple Silicon Mac, and
      # the native binary is the one worth installing there.
      if [ "$arch" = "x64" ] && [ "$(sysctl -n hw.optional.arm64 2>/dev/null || true)" = "1" ]; then
        arch="arm64"
      fi
      platform="darwin-$arch"
      ;;
    Linux) platform="linux-$arch" ;;
    *) die_with_fallback "no rqx binary is published for $os; Windows has install.ps1" ;;
  esac
}

# Every download lands in a file so a failed transfer cannot be mistaken for content.
# HTTPS only, including across redirects.
download() {
  url="$1"
  destination="$2"
  if have curl; then
    curl --proto '=https' --tlsv1.2 --proto-redir '=https' -fsSL -o "$destination" "$url"
  elif have wget; then
    wget --https-only --secure-protocol=TLSv1_2 -q -O "$destination" "$url"
  else
    die "neither curl nor wget is installed"
  fi
}

# The final URL of /releases/latest ends in /tag/vX.Y.Z. Reading it from the
# redirect keeps this off the GitHub API and its rate limit.
resolve_latest_url() {
  latest="$RELEASES_URL/latest"
  if have curl; then
    curl --proto '=https' --tlsv1.2 --proto-redir '=https' -fsSL \
      -o /dev/null -w '%{url_effective}' "$latest"
  elif have wget; then
    wget --https-only --secure-protocol=TLSv1_2 -q --server-response --spider "$latest" 2>&1 |
      sed -n 's/^ *[Ll]ocation: *//p' | tail -n 1
  else
    die "neither curl nor wget is installed"
  fi
}

resolve_latest_version() {
  final_url="$(resolve_latest_url)" || die_with_fallback "could not reach $RELEASES_URL/latest"
  case "$final_url" in
    */releases/tag/*) tag="${final_url##*/}" ;;
    *) die_with_fallback "$REPOSITORY has no published release yet" ;;
  esac
  version="${tag#v}"
  case "$version" in
    *-*) die "the latest release is the prerelease $version; name it with --version to install it" ;;
  esac
}

check_version() {
  case "$version" in
    [0-9]*.[0-9]*.[0-9]* ) ;;
    *) die "\`$version\` is not a version like 1.2.3" ;;
  esac
  case "$version" in
    *[!0-9A-Za-z.-]*) die "\`$version\` is not a version like 1.2.3" ;;
  esac
}

sha256_of() {
  if have sha256sum; then
    sha256sum "$1" | cut -d ' ' -f 1
  elif have shasum; then
    shasum -a 256 "$1" | cut -d ' ' -f 1
  else
    die "neither sha256sum nor shasum is installed, so the download cannot be verified"
  fi
}

verify_checksum() {
  archive_path="$1"
  checksums_path="$2"
  archive_name="$3"

  # Only this archive's line is read: `sha256sum --check` would want every other
  # platform's tarball on disk as well.
  expected="$(awk -v name="$archive_name" '$2 == name || $2 == "*" name { print $1 }' "$checksums_path")"
  case "$expected" in
    *[!0-9a-f]* | "") die "SHA256SUMS has no valid checksum for $archive_name; nothing was installed" ;;
  esac
  [ "${#expected}" -eq 64 ] || die "SHA256SUMS has no valid checksum for $archive_name; nothing was installed"

  actual="$(sha256_of "$archive_path")"
  if [ "$actual" != "$expected" ]; then
    die "$archive_name hashes to $actual but SHA256SUMS says $expected; nothing was installed"
  fi
}

print_path_hint() {
  case ":${PATH:-}:" in
    *":$install_dir:"*) ;;
    *)
      echo
      echo "$install_dir is not on your PATH. To add it for this shell, run:"
      echo "  export PATH=\"$install_dir:\$PATH\""
      echo "Add that line to your shell's startup file yourself if you want it permanent."
      ;;
  esac
}

main() {
  parse_arguments "$@"
  [ -n "$install_dir" ] || die "no install directory: HOME is unset, so pass --dir"
  detect_target

  if [ -z "$version" ]; then
    resolve_latest_version
  fi
  check_version

  archive_name="rqx-$version-$platform.tar.gz"
  download_base="$RELEASES_URL/download/v$version"
  archive_url="$download_base/$archive_name"
  checksums_url="$download_base/SHA256SUMS"
  target="$install_dir/rqx"

  if [ -n "$dry_run" ]; then
    echo "rqx install, dry run: nothing will be downloaded or changed"
    step "version    $version"
    step "platform   $platform"
    step "archive    $archive_url"
    step "checksums  $checksums_url"
    step "verify     $archive_name against SHA256SUMS before installing"
    step "install    $target (mode 755, directory created if missing)"
    exit 0
  fi

  work_dir="$(mktemp -d "${TMPDIR:-/tmp}/rqx-install.XXXXXX")"
  trap cleanup EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM

  echo "Installing rqx $version for $platform"
  step "downloading $archive_url"
  download "$archive_url" "$work_dir/$archive_name" ||
    die_with_fallback "could not download $archive_url (no binary published for v$version on $platform, or the network failed)"
  step "downloading $checksums_url"
  download "$checksums_url" "$work_dir/SHA256SUMS" ||
    die_with_fallback "could not download $checksums_url"

  step "verifying checksum"
  verify_checksum "$work_dir/$archive_name" "$work_dir/SHA256SUMS" "$archive_name"

  mkdir "$work_dir/extracted"
  tar -xzf "$work_dir/$archive_name" -C "$work_dir/extracted" rqx ||
    die "$archive_name does not contain an rqx binary; nothing was installed"

  mkdir -p "$install_dir"
  install -m 755 "$work_dir/extracted/rqx" "$target"
  step "installed $target"

  if installed_version="$("$target" --version 2>&1)"; then
    echo "$installed_version"
  else
    echo "warning: installed $target but it did not run: $installed_version" >&2
  fi
  print_path_hint
}

main "$@"
