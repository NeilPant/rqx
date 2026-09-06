"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.REQUIRED_VERSION = void 0;
exports.resolveBinary = resolveBinary;
exports.warnOnVersionMismatch = warnOnVersionMismatch;
const vscode_1 = require("vscode");
const node_crypto_1 = require("node:crypto");
const promises_1 = require("node:fs/promises");
const node_child_process_1 = require("node:child_process");
const node_os_1 = require("node:os");
const node_path_1 = require("node:path");
const RELEASES = "https://github.com/jadhamwi21/rqx/releases/download";
/// Bumped with the extension whenever it starts relying on a server capability. The
/// server reports its own version in `serverInfo`, so a mismatch is a message rather
/// than a confusing failure three requests later.
exports.REQUIRED_VERSION = "0.1.0";
const TARGETS = {
    "darwin-arm64": "aarch64-apple-darwin",
    "darwin-x64": "x86_64-apple-darwin",
    "linux-arm64": "aarch64-unknown-linux-gnu",
    "linux-x64": "x86_64-unknown-linux-gnu",
    "win32-x64": "x86_64-pc-windows-msvc",
};
function target() {
    return TARGETS[`${(0, node_os_1.platform)()}-${(0, node_os_1.arch)()}`];
}
/// `activate` waits on this, so a candidate that hangs instead of answering — a stale
/// network mount, a binary waiting on stdin — would leave the extension starting forever.
function runs(command) {
    return new Promise((resolve) => {
        (0, node_child_process_1.execFile)(command, ["--version"], { timeout: 10_000 }, (error) => resolve(!error));
    });
}
/// The platform builds carry the matching server, because that is the only path that
/// works with no network, no toolchain and no PATH entry.
async function shipped(context) {
    const binary = (0, node_path_1.join)(context.extensionUri.fsPath, "server", (0, node_os_1.platform)() === "win32" ? "rqx.exe" : "rqx");
    // A VSIX is unpacked without its file modes, so the shipped binary arrives unexecutable.
    await (0, promises_1.chmod)(binary, 0o755).catch(() => undefined);
    return (await runs(binary)) ? binary : undefined;
}
/// `activate` awaits this, so a stalled CDN with no deadline leaves the extension
/// activating forever with no way out.
async function fetchOrThrow(url) {
    const response = await fetch(url, { signal: AbortSignal.timeout(60_000) });
    if (!response.ok) {
        throw new Error(`${url} returned ${response.status}`);
    }
    return response.arrayBuffer();
}
/// Both the archive and the sums come from the same origin over the same channel, so
/// this catches a truncated or corrupted download rather than a substituted release —
/// TLS is what makes the origin trustworthy.
async function verify(bytes, url, asset) {
    const sums = new TextDecoder().decode(await fetchOrThrow(`${url}/SHA256SUMS`));
    const expected = sums
        .split("\n")
        .map((line) => line.trim().split(/\s+/))
        .find(([, name]) => name?.endsWith(asset))?.[0];
    if (!expected) {
        throw new Error(`SHA256SUMS names no ${asset}`);
    }
    const actual = (0, node_crypto_1.createHash)("sha256").update(bytes).digest("hex");
    if (actual !== expected) {
        throw new Error(`checksum mismatch for ${asset}`);
    }
}
async function download(context, log) {
    const slug = target();
    if (!slug) {
        log.appendLine(`no published build for ${(0, node_os_1.platform)()}-${(0, node_os_1.arch)()}`);
        return undefined;
    }
    const directory = (0, node_path_1.join)(context.globalStorageUri.fsPath, exports.REQUIRED_VERSION);
    const binary = (0, node_path_1.join)(directory, (0, node_os_1.platform)() === "win32" ? "rqx.exe" : "rqx");
    if (await runs(binary)) {
        return binary;
    }
    const asset = `rqx-${exports.REQUIRED_VERSION}-${slug}.tar.gz`;
    const url = `${RELEASES}/v${exports.REQUIRED_VERSION}`;
    log.appendLine(`downloading ${asset}`);
    await vscode_1.window.withProgress({ location: vscode_1.ProgressLocation.Window, title: "Rqx: fetching the language server" }, async () => {
        const archive = Buffer.from(await fetchOrThrow(`${url}/${asset}`));
        await verify(archive, url, asset);
        await (0, promises_1.mkdir)(directory, { recursive: true });
        const tarball = (0, node_path_1.join)(directory, asset);
        await (0, promises_1.writeFile)(tarball, archive);
        await extract(tarball, directory);
        await (0, promises_1.rm)(tarball, { force: true });
        await (0, promises_1.chmod)(binary, 0o755);
    });
    return (await runs(binary)) ? binary : undefined;
}
function extract(tarball, into) {
    return new Promise((resolve, reject) => {
        // Every supported platform ships bsdtar as `tar`, Windows included since 1803,
        // so unpacking needs no dependency of our own.
        (0, node_child_process_1.execFile)("tar", ["-xzf", tarball, "-C", into, "--strip-components", "1"], (error) => error ? reject(error) : resolve());
    });
}
/// An explicitly configured path wins, then whatever is on PATH — silently preferring
/// our own copy over the one someone installed on purpose is how a tool starts
/// disagreeing with its own CI — then the build shipped inside this extension, then a
/// managed download for the universal package that ships without one.
async function resolveBinary(context, log) {
    const configured = vscode_1.workspace.getConfiguration("rqx").get("path", "rqx").trim();
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
    }
    catch (error) {
        log.appendLine(`could not fetch a binary: ${String(error)}`);
        return undefined;
    }
}
async function warnOnVersionMismatch(reported, log) {
    if (!reported || reported === exports.REQUIRED_VERSION) {
        return;
    }
    log.appendLine(`server is ${reported}, extension expects ${exports.REQUIRED_VERSION}`);
    const choice = await vscode_1.window.showWarningMessage(`Rqx ${reported} is older than this extension expects (${exports.REQUIRED_VERSION}). Some features will be missing.`, "Show Log");
    if (choice === "Show Log") {
        log.show();
    }
}
//# sourceMappingURL=binary.js.map