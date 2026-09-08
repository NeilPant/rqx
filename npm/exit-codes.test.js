"use strict";

const assert = require("node:assert");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { spawnSync } = require("node:child_process");
const { test } = require("node:test");

const SHIM = path.join(__dirname, "bin", "rqx");
const PLATFORM = `${process.platform}-${process.arch}`;

// node stands in for the binary, because `-e "process.exit(n)"` is the only way to
// make a child exit with a chosen code on every platform the shim runs on.
function tree({ withPlatformPackage }) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "rqx-shim-"));
  const home = path.join(root, "node_modules", "@rqx");

  const cli = path.join(home, "cli", "bin");
  fs.mkdirSync(cli, { recursive: true });
  fs.copyFileSync(SHIM, path.join(cli, "rqx"));

  if (withPlatformPackage) {
    const platform = path.join(home, PLATFORM);
    fs.mkdirSync(platform, { recursive: true });
    fs.writeFileSync(
      path.join(platform, "package.json"),
      JSON.stringify({ name: `@rqx/${PLATFORM}`, version: "0.0.0" }),
    );
    const binary = path.join(
      platform,
      process.platform === "win32" ? "rqx.exe" : "rqx",
    );
    if (process.platform === "win32") fs.copyFileSync(process.execPath, binary);
    else fs.symlinkSync(process.execPath, binary);
  }

  return path.join(cli, "rqx");
}

function run(shim, args) {
  return spawnSync(process.execPath, [shim, ...args], { encoding: "utf8" });
}

test("the child's exit code reaches the caller untouched", () => {
  const shim = tree({ withPlatformPackage: true });
  for (const code of [0, 1, 2, 101]) {
    const { status } = run(shim, ["-e", `process.exit(${code})`]);
    assert.strictEqual(status, code, `exit ${code} did not survive the shim`);
  }
});

test("a child killed by a signal exits 128 + the signal", () => {
  const shim = tree({ withPlatformPackage: true });
  const { status } = run(shim, [
    "-e",
    "process.kill(process.pid, 'SIGTERM')",
  ]);
  assert.strictEqual(status, 128 + os.constants.signals.SIGTERM);
});

test("a missing platform package exits 2 and names it", () => {
  const shim = tree({ withPlatformPackage: false });
  const { status, stderr } = run(shim, ["--version"]);
  assert.strictEqual(status, 2);
  assert.match(stderr, new RegExp(`@rqx/${PLATFORM}`));
});

test("the child is told a package manager owns this install", () => {
  const shim = tree({ withPlatformPackage: true });
  const { stdout } = run(shim, ["-p", "process.env.RQX_MANAGED"]);
  assert.strictEqual(stdout.trim(), "npm");
});
