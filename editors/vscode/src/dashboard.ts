import { ProgressLocation, Uri, commands, env, window, workspace } from "vscode";

/// VS Code's own Simple Browser, which every build has and no extension has to ship.
const SIMPLE_BROWSER = "simpleBrowser.show";

type Answer = { url?: string; error?: string };

/// The dashboard is one page served by one process, and the language server owns it:
/// asking twice gets the same URL rather than a second server on a second port.
export class Dashboard {
  constructor(private readonly ask: (command: string) => Promise<Answer | undefined>) {}

  public async open(inEditor: boolean): Promise<void> {
    const url = await this.url();
    if (!url) {
      return;
    }
    // A remote or Codespaces window reaches a loopback port only through a forwarded
    // one, and `openExternal` resolves that itself while the Simple Browser does not.
    if (!inEditor) {
      await env.openExternal(Uri.parse(url));
      return;
    }
    const external = await env.asExternalUri(Uri.parse(url));
    await commands.executeCommand(SIMPLE_BROWSER, external.toString());
  }

  private async url(): Promise<string | undefined> {
    // The first call waits for the dashboard's own first scan, which on a large
    // repository is seconds of apparent silence.
    const answer = await window.withProgress(
      { location: ProgressLocation.Window, title: "Rqx: starting the dashboard" },
      () => this.ask("rqx.dashboard"),
    );
    if (answer?.url) {
      return answer.url;
    }
    const reason = answer?.error ?? "the server did not answer";
    const choice = await window.showErrorMessage(
      `Rqx: the dashboard did not start — ${reason}.`,
      "Show Log",
    );
    if (choice === "Show Log") {
      await commands.executeCommand("rqx.showOutput");
    }
    return undefined;
  }
}

export function opensInEditor(): boolean {
  return workspace.getConfiguration("rqx").get<string>("dashboard.openIn", "editor") === "editor";
}
