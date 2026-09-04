import {
  ExtensionContext,
  OutputChannel,
  StatusBarAlignment,
  StatusBarItem,
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

interface WorkspaceReport {
  health: number;
  findings: number;
  files: number;
  elapsedMs: number;
}

const LANGUAGES = ["typescript", "typescriptreact", "python", "go", "java", "cpp", "c"];

let client: LanguageClient | undefined;
let status: StatusBarItem;
let log: OutputChannel;

function serverOptions(): ServerOptions {
  const settings = workspace.getConfiguration("rqx");
  const command = settings.get<string>("path", "rqx");
  const args = ["lsp"];
  const configured = settings.get<string>("configFile", "").trim();
  if (configured) {
    args.push("--config", resolveInWorkspace(configured));
  }
  const cwd = workspace.workspaceFolders?.[0]?.uri.fsPath;
  const run: Executable = { command, args, options: cwd ? { cwd } : undefined };
  return { run, debug: run };
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

function showHealth(report: WorkspaceReport): void {
  const icon = report.health >= 80 ? "$(pass)" : report.health >= 50 ? "$(warning)" : "$(error)";
  status.text = `${icon} rqx ${report.health}/100`;
  status.tooltip =
    `${report.findings} findings across ${report.files} files (${report.elapsedMs}ms)\n` +
    `Click to rescan the workspace.`;
  status.show();
}

async function scanWorkspace(): Promise<void> {
  if (!client) {
    return;
  }
  status.text = "$(sync~spin) rqx scanning";
  status.show();
  try {
    const report = await client.sendRequest<WorkspaceReport>("rqx/scanWorkspace", {});
    showHealth(report);
  } catch (error) {
    // A failed scan must not leave a spinner on screen implying work is still happening.
    status.text = "$(error) rqx unavailable";
    status.tooltip = String(error);
    status.show();
    log.appendLine(`scan failed: ${String(error)}`);
  }
}

// The server reads its configuration once at startup and holds it for the life of the
// process, so a changed RQX.toml only takes effect on a restart. Restarting the child
// here is one line; a reload path through the server is a protocol, a cache
// invalidation and a race.
async function reload(reason: string): Promise<void> {
  if (!client) {
    return;
  }
  log.appendLine(`restarting: ${reason}`);
  status.text = "$(sync~spin) rqx reloading";
  status.show();
  await client.restart();
  await scanWorkspace();
}

export async function activate(context: ExtensionContext): Promise<void> {
  log = window.createOutputChannel("Rqx");
  status = window.createStatusBarItem(StatusBarAlignment.Right, 100);
  status.command = "rqx.scanWorkspace";
  context.subscriptions.push(status, log);

  const watcher = workspace.createFileSystemWatcher("**/RQX.toml");
  const clientOptions: LanguageClientOptions = {
    documentSelector: LANGUAGES.map((language) => ({ scheme: "file", language })),
    outputChannel: log,
    synchronize: { fileEvents: watcher },
  };

  client = new LanguageClient("rqx", "Rqx", serverOptions(), clientOptions);

  context.subscriptions.push(
    watcher,
    watcher.onDidChange((uri) => reload(`${uri.fsPath} changed`)),
    watcher.onDidCreate((uri) => reload(`${uri.fsPath} added`)),
    watcher.onDidDelete((uri) => reload(`${uri.fsPath} removed`)),
    workspace.onDidChangeConfiguration((event) => {
      if (event.affectsConfiguration("rqx.path") || event.affectsConfiguration("rqx.configFile")) {
        reload("settings changed");
      }
    }),
    commands.registerCommand("rqx.scanWorkspace", scanWorkspace),
    commands.registerCommand("rqx.restart", () => reload("asked to restart")),
    commands.registerCommand("rqx.showOutput", () => log.show()),
  );

  try {
    await client.start();
  } catch (error) {
    const binary = workspace.getConfiguration("rqx").get<string>("path", "rqx");
    status.text = "$(error) rqx not found";
    status.tooltip = String(error);
    status.show();
    log.appendLine(`could not start \`${binary} lsp\`: ${String(error)}`);
    const choice = await window.showErrorMessage(
      `Rqx could not start \`${binary}\`. Install it with \`cargo install rqx\`, or point "rqx.path" at the binary.`,
      "Open Settings",
      "Show Log",
    );
    if (choice === "Open Settings") {
      await commands.executeCommand("workbench.action.openSettings", "rqx.path");
    } else if (choice === "Show Log") {
      log.show();
    }
    return;
  }

  if (workspace.getConfiguration("rqx").get<boolean>("scanOnStartup", true)) {
    await scanWorkspace();
  }
}

export function deactivate(): Promise<void> | undefined {
  return client?.stop();
}
