#!/usr/bin/env bash
# Captures the dashboard for the README. Renders the static report first, so the
# capture needs no server and no port: `--reporter html` writes the same page
# `rqx serve` hands out, with every payload already in it.
#
#   tools/screenshots.sh            # scan this repository
#   tools/screenshots.sh ../other   # scan somewhere else
#
# Needs Playwright's chromium: `npx playwright install chromium`.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
target="${1:-$root}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

cargo build --release --manifest-path "$root/Cargo.toml"
"$root/target/release/rqx" scan "$target" --reporter "html=$work/report.html" --quiet >/dev/null

cat > "$work/shoot.mjs" <<'JS'
import { chromium } from 'playwright';

const [page, out] = [process.argv[2], process.argv[3]];
const shots = [
  ['dashboard', ''],
  ['findings', '#findings'],
  ['architecture', '#graph'],
];

const browser = await chromium.launch();
const view = await browser.newPage({
  viewport: { width: 1440, height: 900 },
  deviceScaleFactor: 2,
  colorScheme: 'dark',
});
for (const [name, hash] of shots) {
  await view.goto(`file://${page}${hash}`);
  // The page draws from data already in the document, so one frame is enough;
  // the wait is for the fonts, which decode off the main thread.
  await view.waitForTimeout(600);
  await view.screenshot({ path: `${out}/${name}.png` });
}
await browser.close();
JS

npx --yes playwright@latest install chromium >/dev/null 2>&1 || true
node "$work/shoot.mjs" "$work/report.html" "$root/assets"
echo "wrote assets/dashboard.png, assets/findings.png, assets/architecture.png"
