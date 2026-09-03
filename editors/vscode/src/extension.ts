import { ExtensionContext, StatusBarAlignment, StatusBarItem, commands, window, workspace } from "vscode";
import { Executable, LanguageClient, LanguageClientOptions, ServerOptions } from "vscode-languageclient/node";

interface WorkspaceReport {
  health: number;
  findings: number;
  files: number;
  elapsedMs: number;
}

let client: LanguageClient | undefined;
let status: StatusBarItem;

function serverOptions(): ServerOptions {
  const command = workspace.getConfiguration("rqx").get<string>("path", "rqx");
  const run: Executable = { command, args: ["lsp"] };
  return { run, debug: run };
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
  }
}

export async function activate(context: ExtensionContext): Promise<void> {
  status = window.createStatusBarItem(StatusBarAlignment.Right, 100);
  status.command = "rqx.scanWorkspace";
  context.subscriptions.push(status);

  const clientOptions: LanguageClientOptions = {
    documentSelector: [
      { scheme: "file", language: "typescript" },
      { scheme: "file", language: "typescriptreact" }
    ],
    synchronize: { fileEvents: workspace.createFileSystemWatcher("**/rqx.toml") }
  };

  client = new LanguageClient("rqx", "Rqx", serverOptions(), clientOptions);

  context.subscriptions.push(
    commands.registerCommand("rqx.scanWorkspace", scanWorkspace),
    commands.registerCommand("rqx.restart", async () => {
      await client?.restart();
      await scanWorkspace();
    })
  );

  try {
    await client.start();
  } catch (error) {
    window.showErrorMessage(
      `Rqx could not start. Is the binary on your PATH, or set "rqx.path"? (${error})`
    );
    return;
  }

  if (workspace.getConfiguration("rqx").get<boolean>("scanOnStartup", true)) {
    await scanWorkspace();
  }
}

export function deactivate(): Promise<void> | undefined {
  return client?.stop();
}
