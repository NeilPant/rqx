import {
  Diagnostic,
  DiagnosticSeverity,
  ExtensionContext,
  ThemeColor,
  OutputChannel,
  StatusBarAlignment,
  StatusBarItem,
  Uri,
  ViewColumn,
  commands,
  window,
  workspace,
} from "vscode";
import {
  Executable,
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";
import { isAbsolute, join } from "node:path";

import { resolveBinary, warnOnVersionMismatch } from "./binary";
import { Panel, Report } from "./panel";

interface Health {
  health: number;
  findings: number;
  files: number;
  elapsedMs: number;
}

const LANGUAGES = ["typescript", "typescriptreact", "python", "go", "java", "cpp"];

let client: LanguageClient | undefined;
let context: ExtensionContext;
let status: StatusBarItem;
let log: OutputChannel;
let panel: Panel;

function settings() {
  return workspace.getConfiguration("rqx");
}

// The server is a child process, so a relative path would resolve against whatever
// directory the editor was launched from rather than against the project.
function resolveInWorkspace(path: string): string {
  const root = workspace.workspaceFolders?.[0]?.uri.fsPath;
  if (!root || isAbsolute(path)) {
    return path;
  }
  return join(root, path);
}

function initializationOptions(): Record<string, unknown> {
  const configured = settings().get<string>("configFile", "").trim();
  return {
    configFile: configured ? resolveInWorkspace(configured) : undefined,
    scanOnStartup: settings().get<boolean>("scanOnStartup", true),
    scanOnSave: settings().get<boolean>("scanOnSave", true),
    codeLens: settings().get<boolean>("codeLens", false),
    patterns: settings().get<boolean>("patterns", false),
  };
}

/// `vscode.DiagnosticSeverity` counts up as severity falls — Error 0 through Hint 3 —
/// so the cap is a floor on the number.
function quieten(diagnostic: Diagnostic, cap: DiagnosticSeverity): Diagnostic {
  const severity = Math.max(diagnostic.severity, cap);
  if (severity === diagnostic.severity) {
    return diagnostic;
  }
  const quiet = new Diagnostic(diagnostic.range, diagnostic.message, severity);
  quiet.code = diagnostic.code;
  quiet.source = diagnostic.source;
  quiet.relatedInformation = diagnostic.relatedInformation;
  quiet.tags = diagnostic.tags;
  return quiet;
}

/// rqx is a second opinion alongside the language's own server, and on an existing
/// codebase its first run is long. Capping the severity lets someone keep working
/// through the list without the Problems panel drowning what their compiler said.
function severityCap(): DiagnosticSeverity | undefined {
  switch (settings().get<string>("severity", "asReported")) {
    case "warning":
      return DiagnosticSeverity.Warning;
    case "information":
      return DiagnosticSeverity.Information;
    case "hint":
      return DiagnosticSeverity.Hint;
    default:
      return undefined;
  }
}

function serverOptions(command: string): ServerOptions {
  const cwd = workspace.workspaceFolders?.[0]?.uri.fsPath;
  const run: Executable = {
    command,
    args: ["lsp"],
    options: cwd ? { cwd } : undefined,
  };
  return { run, debug: run };
}

let latest: Health | undefined;
let report: Report | undefined;
/// The score the last notification was measured against, so a repository that stays bad
/// is reported once rather than on every save.
let announced: number | undefined;

function showHealth(report: Health): void {
  const previous = latest?.health;
  latest = report;

  const mode = settings().get<string>("statusBar", "health");
  if (mode === "off") {
    status.hide();
    return;
  }
  const icon = "$(rqx-owl)";
  // The owl is the identity and stays put; the verdict moves to the background, which
  // is the one thing in a status bar that reads without being looked at.
  status.backgroundColor =
    report.health < 50
      ? new ThemeColor("statusBarItem.errorBackground")
      : report.health < 80
        ? new ThemeColor("statusBarItem.warningBackground")
        : undefined;
  // An arrow only where there is a previous score to compare against; a trend shown on
  // the first scan of a session would be invented.
  const drift =
    previous === undefined || previous === report.health
      ? ""
      : report.health > previous
        ? ` $(arrow-up)${report.health - previous}`
        : ` $(arrow-down)${previous - report.health}`;
  status.text =
    mode === "compact"
      ? `${icon} ${report.health}`
      : `${icon} rqx ${report.health}/100${drift}`;
  status.tooltip =
    `${report.findings} findings across ${report.files} files (${report.elapsedMs}ms)\n` +
    `Click to open the panel.`;
  status.show();

  announce(previous, report.health);
}

/// The one thing worth interrupting for: the project got measurably worse, just now,
/// probably because of the save that triggered this scan.
function announce(previous: number | undefined, health: number): void {
  const threshold = settings().get<number>("notifyOnHealthDrop", 5);
  if (threshold <= 0 || previous === undefined) {
    return;
  }
  const drop = previous - health;
  if (drop < threshold || announced === health) {
    return;
  }
  announced = health;
  void window
    .showWarningMessage(`Rqx: health fell ${drop} points, to ${health}/100.`, "Open Panel", "Review the Diff")
    .then((choice) => {
      if (choice === "Open Panel") {
        panel.reveal();
      } else if (choice === "Review the Diff") {
        void check();
      }
    });
}

async function send<T>(method: string, ...args: unknown[]): Promise<T | undefined> {
  if (!client) {
    return undefined;
  }
  try {
    return await client.sendRequest<T>("workspace/executeCommand", {
      command: method,
      arguments: args,
    });
  } catch (error) {
    log.appendLine(`${method} failed: ${String(error)}`);
    window.showWarningMessage(
      `Rqx: ${method} could not run — the server is busy or its scanner stopped. See the log.`,
    );
    return undefined;
  }
}

function scanning(message: string): void {
  status.text = `$(sync~spin) rqx ${message}`;
  status.show();
}

async function scanWorkspace(): Promise<void> {
  scanning("scanning");
  try {
    await send("rqx.scan");
  } finally {
    settle();
  }
}

async function check(): Promise<void> {
  scanning("reviewing");
  let verdict;
  try {
    verdict = await send<{ passed: boolean; reviewedFindings: number; base: string }>(
      "rqx.check",
    );
  } finally {
    settle();
  }
  if (verdict?.base === undefined) {
    return;
  }
  const outcome = verdict.passed ? "passes the gate" : "fails the gate";
  window.showInformationMessage(
    `Rqx: ${verdict.reviewedFindings} findings against ${verdict.base} — ${outcome}.`,
  );
}

/// A spinner is cleared by whatever started it: waiting on the health notification
/// leaves it turning forever when the command answers without one.
function settle(): void {
  if (latest) {
    showHealth(latest);
  } else if (client) {
    status.text = "$(rqx-owl) rqx";
    status.show();
  }
}

/// `rqx explain` already renders the whole catalogue entry, so the panel opens that
/// markdown rather than paraphrasing it into a second, thinner description.
async function explain(): Promise<void> {
  const editor = window.activeTextEditor;
  if (!editor || !client) {
    return;
  }
  const slug = ruleAt(editor.document.uri, editor.selection.active.line);
  if (!slug) {
    window.showInformationMessage("Rqx: no finding on this line.");
    return;
  }
  const answer = await send<{ markdown?: string }>("rqx.explain", slug);
  if (!answer?.markdown) {
    return;
  }
  const document = await workspace.openTextDocument({
    content: answer.markdown,
    language: "markdown",
  });
  await window.showTextDocument(document, { viewColumn: ViewColumn.Beside, preview: true });
  await commands.executeCommand("markdown.showPreview");
}

function ruleAt(uri: Uri, line: number): string | undefined {
  // Pattern advice publishes under its own source with an advisory slug as its code,
  // and asking the server to explain one answers with an error nobody sees.
  const diagnostic = client?.diagnostics
    ?.get(uri)
    ?.find(
      (entry) =>
        entry.source === "rqx" &&
        entry.range.start.line <= line &&
        line <= entry.range.end.line,
    );
  const code = diagnostic?.code;
  if (typeof code === "string") {
    return code;
  }
  return typeof code === "object" && code !== null ? String(code.value) : undefined;
}

async function disableRule(): Promise<void> {
  const editor = window.activeTextEditor;
  if (!editor) {
    return;
  }
  const slug = ruleAt(editor.document.uri, editor.selection.active.line);
  if (!slug) {
    window.showInformationMessage("Rqx: no finding on this line.");
    return;
  }
  await send("rqx.disableRule", slug);
}

async function toggle(command: string, setting: string): Promise<void> {
  const answer = await send<Record<string, boolean>>(command);
  const on = answer?.[setting];
  if (on !== undefined) {
    // Written back so the choice survives a restart; the server holds it only for the
    // life of the process.
    await settings().update(setting, on, true);
    window.showInformationMessage(`Rqx: ${setting} ${on ? "on" : "off"}.`);
  }
}

async function start(context: ExtensionContext): Promise<void> {
  const binary = await resolveBinary(context, log);
  if (!binary) {
    status.text = "$(rqx-owl) rqx not found";
    status.tooltip = "No rqx binary could be found or downloaded.";
    status.show();
    const choice = await window.showErrorMessage(
      "Rqx could not find or download a binary. Retry if you were offline, install it with `cargo install rqx`, or point \"rqx.path\" at one.",
      "Retry",
      "Open Settings",
      "Show Log",
    );
    if (choice === "Retry") {
      await start(context);
    } else if (choice === "Open Settings") {
      await commands.executeCommand("workbench.action.openSettings", "rqx.path");
    } else if (choice === "Show Log") {
      log.show();
    }
    return;
  }

  const clientOptions: LanguageClientOptions = {
    documentSelector: LANGUAGES.map((language) => ({ scheme: "file", language })),
    outputChannel: log,
    initializationOptions: initializationOptions(),
    synchronize: {
      fileEvents: workspace.createFileSystemWatcher("**/RQX.toml"),
    },
    middleware: {
      // Applied on the way in rather than asked of the server: the severity a rule
      // declares is part of the catalogue and the same in CI, so this is the client
      // choosing how loudly to render it, not rqx changing its mind.
      handleDiagnostics: (uri, diagnostics, next) => {
        const cap = severityCap();
        if (cap === undefined) {
          next(uri, diagnostics);
          return;
        }
        next(uri, diagnostics.map((diagnostic) => quieten(diagnostic, cap)));
      },
    },
  };

  client = new LanguageClient("rqx", "Rqx", serverOptions(binary), clientOptions);
  client.onNotification("rqx/health", (health: Health) => showHealth(health));
  client.onNotification("rqx/report", (fresh: Report) => {
    report = fresh;
    panel.show(fresh);
  });

  try {
    await client.start();
  } catch (error) {
    client = undefined;
    log.appendLine(`the server did not start: ${String(error)}`);
    status.text = "$(rqx-owl) rqx stopped";
    status.tooltip = "The rqx language server did not start.";
    status.show();
    const choice = await window.showErrorMessage(
      `Rqx: \`${binary} lsp\` did not start. See the log for what it said.`,
      "Show Log",
      "Retry",
    );
    if (choice === "Show Log") {
      log.show();
    } else if (choice === "Retry") {
      await start(context);
    }
    return;
  }
  await warnOnVersionMismatch(client.initializeResult?.serverInfo?.version, log);
  const catalogue = await send<Parameters<Panel["setCatalogue"]>[0]>("rqx.catalogue");
  if (catalogue) {
    panel.setCatalogue(catalogue);
  }
}

export async function activate(extension: ExtensionContext): Promise<void> {
  context = extension;
  log = window.createOutputChannel("Rqx");
  status = window.createStatusBarItem(StatusBarAlignment.Right, 100);
  status.command = "rqx.openPanel";
  panel = new Panel(context.extensionUri);

  context.subscriptions.push(
    status,
    log,
    window.registerWebviewViewProvider(Panel.id, panel),
    commands.registerCommand("rqx.scanWorkspace", scanWorkspace),
    commands.registerCommand("rqx.check", check),
    commands.registerCommand("rqx.explain", explain),
    commands.registerCommand("rqx.startHere", startHere),
    commands.registerCommand("rqx.disableRule", disableRule),
    commands.registerCommand("rqx.togglePatterns", () =>
      toggle("rqx.togglePatterns", "patterns"),
    ),
    commands.registerCommand("rqx.toggleCodeLens", () =>
      toggle("rqx.toggleCodeLens", "codeLens"),
    ),
    commands.registerCommand("rqx.openPanel", () => panel.reveal()),
    commands.registerCommand("rqx.restart", () => restart("asked to restart")),
    commands.registerCommand("rqx.showOutput", () => log.show()),
    workspace.onDidChangeConfiguration((event) => {
      // `initializationOptions` is read once at construction, so a setting the server
      // reads needs a new client. The rest are how this side draws what it already has,
      // and restarting for those would throw away a scan to change a colour.
      const restarts = ["path", "configFile", "scanOnStartup", "scanOnSave", "codeLens", "patterns"];
      for (const setting of restarts) {
        if (event.affectsConfiguration(`rqx.${setting}`)) {
          void restart(`rqx.${setting} changed`);
          return;
        }
      }
      if (event.affectsConfiguration("rqx.statusBar") && latest) {
        showHealth(latest);
      }
      if (event.affectsConfiguration("rqx.severity")) {
        void scanWorkspace();
      }
      if (event.affectsConfiguration("rqx.startHereLimit") && report) {
        panel.show(report);
      }
    }),
  );

  await start(context);
}

async function startHere(): Promise<void> {
  const ranked = await send<{ file: string }[]>("rqx.startHere");
  const first = ranked?.[0]?.file;
  const root = workspace.workspaceFolders?.[0]?.uri;
  if (!first || !root) {
    window.showInformationMessage("Rqx: nothing to open yet — scan the workspace first.");
    return;
  }
  const document = await workspace.openTextDocument(Uri.joinPath(root, first));
  await window.showTextDocument(document);
  panel.reveal();
}

/// Runs with no client too, so pointing `rqx.path` at a real binary recovers a session
/// that started without one instead of needing the window reloaded.
async function restart(reason: string): Promise<void> {
  log.appendLine(`restarting: ${reason}`);
  scanning("reloading");
  try {
    // A new client rather than `restart()`, because the options are captured when the
    // client is constructed and a restart would send the old ones again.
    await client?.stop();
    client = undefined;
    await start(context);
  } finally {
    settle();
  }
}

export function deactivate(): Promise<void> | undefined {
  return client?.stop();
}
