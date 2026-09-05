import { ExtensionContext, OutputChannel, commands, window, workspace } from "vscode";

import { Filters } from "./filters";
import { FindingsIndex, Report } from "./findings-index";
import { Health, LanguageServer } from "./language-server";
import { Panel } from "./panel";
import { StatusBar } from "./status-bar";
import { register } from "./commands";
import { FilesTree } from "./views/files-tree";
import { FindingsTree } from "./views/findings-tree";
import { RulesTree } from "./views/rules-tree";

let server: LanguageServer | undefined;

export async function activate(context: ExtensionContext): Promise<void> {
  const log: OutputChannel = window.createOutputChannel("Rqx");
  const index = new FindingsIndex();
  const filters = new Filters(context);
  const panel = new Panel(context.extensionUri);
  const findings = new FindingsTree(index, filters);
  const files = new FilesTree(index);
  const rules = new RulesTree(index);

  const status = new StatusBar((drop, health) => announce(drop, health, panel));

  const connection = new LanguageServer(context, log, index, {
    onHealth: (health: Health) => status.show(health),
    onReport: (report: Report) => {
      panel.show(report);
      // The welcome view offers a scan until there is something to show instead.
      void commands.executeCommand("setContext", "rqx.scanned", true);
    },
    onStateChange: () => status.settle(connection.running),
  });
  server = connection;

  context.subscriptions.push(
    status.disposable,
    log,
    window.registerWebviewViewProvider(Panel.id, panel),
    findings.register(),
    files.register(),
    rules.register(),
    ...register({
      server: connection,
      index,
      filters,
      panel,
      findings,
      files,
      rules,
      scanning: (message) => status.working(message),
      settle: () => status.settle(connection.running),
      showLog: () => log.show(),
    }),
    workspace.onDidChangeConfiguration((event) => {
      const restart = connection.restartsFor((section) => event.affectsConfiguration(section));
      if (restart) {
        void reload(status, connection, `${restart} changed`);
        return;
      }
      // The rest are how this side draws what it already has, and restarting for those
      // would throw away a scan to change a colour.
      if (event.affectsConfiguration("rqx.statusBar") && status.health) {
        status.show(status.health);
      }
      if (event.affectsConfiguration("rqx.severity")) {
        void commands.executeCommand("rqx.scanWorkspace");
      }
      if (event.affectsConfiguration("rqx.startHereLimit") && index.report) {
        panel.show(index.report);
      }
      if (event.affectsConfiguration("rqx.findings")) {
        findings.refresh();
      }
    }),
  );

  await connection.start();
}

async function reload(
  status: StatusBar,
  connection: LanguageServer,
  reason: string,
): Promise<void> {
  status.working("reloading");
  try {
    await connection.restart(reason);
  } finally {
    status.settle(connection.running);
  }
}

function announce(drop: number, health: number, panel: Panel): void {
  void window
    .showWarningMessage(
      `Rqx: health fell ${drop} points, to ${health}/100.`,
      "Open Panel",
      "Review the Diff",
    )
    .then((choice) => {
      if (choice === "Open Panel") {
        panel.reveal();
      } else if (choice === "Review the Diff") {
        void commands.executeCommand("rqx.check");
      }
    });
}

export function deactivate(): Promise<void> | undefined {
  return server?.stop();
}
