import {
  Diagnostic,
  DiagnosticSeverity,
  ExtensionContext,
  OutputChannel,
  Uri,
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
import { FindingsIndex, Report, Rule } from "./findings-index";

export interface Health {
  health: number;
  findings: number;
  files: number;
  elapsedMs: number;
}

const LANGUAGES = ["typescript", "typescriptreact", "python", "go", "java", "cpp"];

/// Settings the server reads at startup. Changing one of these needs a new client,
/// because `initializationOptions` are captured when the client is constructed.
const RESTARTS = ["path", "configFile", "scanOnStartup", "scanOnSave", "codeLens", "patterns"];

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
  // Everything the views read travels on the data, and dropping it here would empty
  // them the moment somebody quietened the Problems panel.
  (quiet as Diagnostic & { data?: unknown }).data = (
    diagnostic as Diagnostic & { data?: unknown }
  ).data;
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

export interface ServerEvents {
  onHealth(health: Health): void;
  onReport(report: Report): void;
  onStateChange(): void;
}

/// The connection to `rqx lsp`: starting it, restarting it when a setting the server
/// reads changes, and asking it questions.
export class LanguageServer {
  private client: LanguageClient | undefined;
  private binary: string | undefined;

  constructor(
    private readonly context: ExtensionContext,
    private readonly log: OutputChannel,
    private readonly index: FindingsIndex,
    private readonly events: ServerEvents,
  ) {}

  public get running(): boolean {
    return this.client !== undefined;
  }

  /// Where the binary the server is running from lives, for the commands that shell
  /// out to it rather than asking over the protocol.
  public get executable(): string | undefined {
    return this.binary;
  }

  public async start(): Promise<void> {
    const binary = await resolveBinary(this.context, this.log);
    if (!binary) {
      this.events.onStateChange();
      await this.offerRecovery();
      return;
    }
    this.binary = binary;

    const clientOptions: LanguageClientOptions = {
      documentSelector: LANGUAGES.map((language) => ({ scheme: "file", language })),
      outputChannel: this.log,
      initializationOptions: initializationOptions(),
      synchronize: {
        fileEvents: workspace.createFileSystemWatcher("**/RQX.toml"),
      },
      middleware: {
        // Applied on the way in rather than asked of the server: the severity a rule
        // declares is part of the catalogue and the same in CI, so this is the client
        // choosing how loudly to render it, not rqx changing its mind.
        handleDiagnostics: (uri, diagnostics, next) => {
          this.index.absorb(uri, diagnostics);
          const cap = severityCap();
          next(uri, cap === undefined ? diagnostics : diagnostics.map((one) => quieten(one, cap)));
        },
      },
    };

    const client = new LanguageClient("rqx", "Rqx", serverOptions(binary), clientOptions);
    client.onNotification("rqx/health", (health: Health) => this.events.onHealth(health));
    client.onNotification("rqx/report", (report: Report) => {
      this.index.setReport(report);
      this.events.onReport(report);
    });

    try {
      await client.start();
      this.client = client;
    } catch (error) {
      this.client = undefined;
      this.log.appendLine(`the server did not start: ${String(error)}`);
      this.events.onStateChange();
      const choice = await window.showErrorMessage(
        `Rqx: \`${binary} lsp\` did not start. See the log for what it said.`,
        "Show Log",
        "Retry",
      );
      if (choice === "Show Log") {
        this.log.show();
      } else if (choice === "Retry") {
        await this.start();
      }
      return;
    }

    await warnOnVersionMismatch(client.initializeResult?.serverInfo?.version, this.log);
    this.events.onStateChange();
    const catalogue = await this.send<Rule[]>("rqx.catalogue");
    if (catalogue) {
      this.index.setCatalogue(catalogue);
    }
  }

  private async offerRecovery(): Promise<void> {
    const choice = await window.showErrorMessage(
      'Rqx could not find or download a binary. Retry if you were offline, install it with `cargo install rqx`, or point "rqx.path" at one.',
      "Retry",
      "Open Settings",
      "Show Log",
    );
    if (choice === "Retry") {
      await this.start();
    } else if (choice === "Open Settings") {
      await commands.executeCommand("workbench.action.openSettings", "rqx.path");
    } else if (choice === "Show Log") {
      this.log.show();
    }
  }

  /// Runs with no client too, so pointing `rqx.path` at a real binary recovers a
  /// session that started without one instead of needing the window reloaded.
  public async restart(reason: string): Promise<void> {
    this.log.appendLine(`restarting: ${reason}`);
    // A new client rather than `restart()`, because the options are captured when the
    // client is constructed and a restart would send the old ones again.
    await this.client?.stop();
    this.client = undefined;
    await this.start();
  }

  public restartsFor(affects: (section: string) => boolean): string | undefined {
    return RESTARTS.map((setting) => `rqx.${setting}`).find((section) => affects(section));
  }

  public async send<T>(method: string, ...args: unknown[]): Promise<T | undefined> {
    if (!this.client) {
      return undefined;
    }
    try {
      return await this.client.sendRequest<T>("workspace/executeCommand", {
        command: method,
        arguments: args,
      });
    } catch (error) {
      this.log.appendLine(`${method} failed: ${String(error)}`);
      window.showWarningMessage(
        `Rqx: ${method} could not run — the server is busy or its scanner stopped. See the log.`,
      );
      return undefined;
    }
  }

  /// The rule reported at a line, read back from what the client was actually told
  /// rather than recomputed.
  public ruleAt(uri: Uri, line: number): string | undefined {
    const diagnostic = this.client?.diagnostics
      ?.get(uri)
      ?.find(
        (entry) =>
          entry.source === "rqx" && entry.range.start.line <= line && line <= entry.range.end.line,
      );
    const code = diagnostic?.code;
    if (typeof code === "string") {
      return code;
    }
    return typeof code === "object" && code !== null ? String(code.value) : undefined;
  }

  public stop(): Promise<void> | undefined {
    return this.client?.stop();
  }
}
