import { ExtensionContext, OutputChannel, ProgressLocation, window, workspace } from "vscode";
import { createHash } from "node:crypto";
import { chmod, mkdir, rm, writeFile } from "node:fs/promises";
import { execFile } from "node:child_process";
import { arch, platform } from "node:os";
import { join } from "node:path";

const RELEASES = "https://github.com/jadhamwi/rqx/releases/download";

/// Bumped with the extension whenever it starts relying on a server capability. The
/// server reports its own version in `serverInfo`, so a mismatch is a message rather
/// than a confusing failure three requests later.
export const REQUIRED_VERSION = "0.1.0";

const TARGETS: Record<string, string> = {
  "darwin-arm64": "aarch64-apple-darwin",
  "darwin-x64": "x86_64-apple-darwin",
  "linux-arm64": "aarch64-unknown-linux-gnu",
  "linux-x64": "x86_64-unknown-linux-gnu",
  "win32-x64": "x86_64-pc-windows-msvc",
};

function target(): string | undefined {
  return TARGETS[`${platform()}-${arch()}`];
}

/// `activate` waits on this, so a candidate that hangs instead of answering — a stale
/// network mount, a binary waiting on stdin — would leave the extension starting forever.
function runs(command: string): Promise<boolean> {
  return new Promise((resolve) => {
    execFile(command, ["--version"], { timeout: 10_000 }, (error) => resolve(!error));
  });
}

/// The platform builds carry the matching server, because that is the only path that
/// works with no network, no toolchain and no PATH entry.
async function shipped(context: ExtensionContext): Promise<string | undefined> {
  const binary = join(
    context.extensionUri.fsPath,
    "server",
    platform() === "win32" ? "rqx.exe" : "rqx",
  );
  // A VSIX is unpacked without its file modes, so the shipped binary arrives unexecutable.
  await chmod(binary, 0o755).catch(() => undefined);
  return (await runs(binary)) ? binary : undefined;
}

/// `activate` awaits this, so a stalled CDN with no deadline leaves the extension
/// activating forever with no way out.
async function fetchOrThrow(url: string): Promise<ArrayBuffer> {
  const response = await fetch(url, { signal: AbortSignal.timeout(60_000) });
  if (!response.ok) {
    throw new Error(`${url} returned ${response.status}`);
  }
  return response.arrayBuffer();
}

/// Both the archive and the sums come from the same origin over the same channel, so
/// this catches a truncated or corrupted download rather than a substituted release —
/// TLS is what makes the origin trustworthy.
async function verify(bytes: Buffer, url: string, asset: string): Promise<void> {
  const sums = new TextDecoder().decode(await fetchOrThrow(`${url}/SHA256SUMS`));
  const expected = sums
    .split("\n")
    .map((line) => line.trim().split(/\s+/))
    .find(([, name]) => name?.endsWith(asset))?.[0];
  if (!expected) {
    throw new Error(`SHA256SUMS names no ${asset}`);
  }
  const actual = createHash("sha256").update(bytes).digest("hex");
  if (actual !== expected) {
    throw new Error(`checksum mismatch for ${asset}`);
  }
}

async function download(
  context: ExtensionContext,
  log: OutputChannel,
): Promise<string | undefined> {
  const slug = target();
  if (!slug) {
    log.appendLine(`no published build for ${platform()}-${arch()}`);
    return undefined;
  }
  const directory = join(context.globalStorageUri.fsPath, REQUIRED_VERSION);
  const binary = join(directory, platform() === "win32" ? "rqx.exe" : "rqx");
  if (await runs(binary)) {
    return binary;
  }

  const asset = `rqx-${REQUIRED_VERSION}-${slug}.tar.gz`;
  const url = `${RELEASES}/v${REQUIRED_VERSION}`;
  log.appendLine(`downloading ${asset}`);
  await window.withProgress(
    { location: ProgressLocation.Window, title: "Rqx: fetching the language server" },
    async () => {
      const archive = Buffer.from(await fetchOrThrow(`${url}/${asset}`));
      await verify(archive, url, asset);

      await mkdir(directory, { recursive: true });
      const tarball = join(directory, asset);
      await writeFile(tarball, archive);
      await extract(tarball, directory);
      await rm(tarball, { force: true });
      await chmod(binary, 0o755);
    },
  );
  return (await runs(binary)) ? binary : undefined;
}

function extract(tarball: string, into: string): Promise<void> {
  return new Promise((resolve, reject) => {
    // Every supported platform ships bsdtar as `tar`, Windows included since 1803,
    // so unpacking needs no dependency of our own.
    execFile("tar", ["-xzf", tarball, "-C", into, "--strip-components", "1"], (error) =>
      error ? reject(error) : resolve(),
    );
  });
}

/// An explicitly configured path wins, then whatever is on PATH — silently preferring
/// our own copy over the one someone installed on purpose is how a tool starts
/// disagreeing with its own CI — then the build shipped inside this extension, then a
/// managed download for the universal package that ships without one.
export async function resolveBinary(
  context: ExtensionContext,
  log: OutputChannel,
): Promise<string | undefined> {
  const configured = workspace.getConfiguration("rqx").get<string>("path", "rqx").trim();
  if (configured && (await runs(configured))) {
    log.appendLine(`using \`${configured}\``);
    return configured;
  }
  if (configured && configured !== "rqx") {
    log.appendLine(`\`${configured}\` did not run; falling back to the shipped build`);
  }
  const bundled = await shipped(context);
  if (bundled) {
    log.appendLine(`using the bundled server at ${bundled}`);
    return bundled;
  }
  try {
    return await download(context, log);
  } catch (error) {
    log.appendLine(`could not fetch a binary: ${String(error)}`);
    return undefined;
  }
}

export async function warnOnVersionMismatch(
  reported: string | undefined,
  log: OutputChannel,
): Promise<void> {
  if (!reported || reported === REQUIRED_VERSION) {
    return;
  }
  log.appendLine(`server is ${reported}, extension expects ${REQUIRED_VERSION}`);
  const choice = await window.showWarningMessage(
    `Rqx ${reported} is older than this extension expects (${REQUIRED_VERSION}). Some features will be missing.`,
    "Show Log",
  );
  if (choice === "Show Log") {
    log.show();
  }
}
