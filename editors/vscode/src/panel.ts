import {
  CancellationToken,
  Uri,
  Webview,
  WebviewView,
  WebviewViewProvider,
  WebviewViewResolveContext,
  commands,
  window,
  workspace,
} from "vscode";
import { randomBytes } from "node:crypto";
import { workspace as config } from "vscode";

export interface Report {
  health: number;
  elapsedMs: number;
  counts: { total: number; by_severity: Record<string, number> };
  grades: Record<string, string>;
  startHere: { file: string; grade: string; findings?: number; importers?: number; commits?: number }[];
}

interface Rule {
  slug: string;
  family: string;
  metric: string;
  why: string;
  citation: string;
  fixes: string[];
}

type Incoming = { type: "ready" } | { type: "open"; file: string } | { type: "scan" };

export class Panel implements WebviewViewProvider {
  public static readonly id = "rqx.panel";

  private view: WebviewView | undefined;
  private report: Report | undefined;
  private catalogue: Rule[] = [];

  constructor(private readonly extension: Uri) {}

  public show(report: Report): void {
    this.report = report;
    this.view?.webview.postMessage({ type: "report", report: this.trimmed(report) });
  }

  /// The server ranks twenty; how many of them are worth a sidebar is the reader's call.
  private trimmed(report: Report): Report {
    const limit = config.getConfiguration("rqx").get<number>("startHereLimit", 10);
    return { ...report, startHere: (report.startHere ?? []).slice(0, limit) };
  }

  public setCatalogue(catalogue: Rule[]): void {
    this.catalogue = catalogue;
    this.view?.webview.postMessage({ type: "catalogue", catalogue });
  }

  public reveal(): void {
    void commands.executeCommand(`${Panel.id}.focus`);
  }

  public resolveWebviewView(
    view: WebviewView,
    _context: WebviewViewResolveContext,
    _token: CancellationToken,
  ): void {
    this.view = view;
    view.webview.options = {
      enableScripts: true,
      localResourceRoots: [Uri.joinPath(this.extension, "media")],
    };
    view.webview.html = this.html(view.webview);
    view.webview.onDidReceiveMessage((message: Incoming) => {
      if (message.type === "ready") {
        this.replay();
      } else if (message.type === "open") {
        void this.open(message.file);
      } else if (message.type === "scan") {
        void commands.executeCommand("rqx.scanWorkspace");
      }
    });
  }

  private replay(): void {
    if (this.report) {
      this.view?.webview.postMessage({ type: "report", report: this.trimmed(this.report) });
    }
    if (this.catalogue.length > 0) {
      this.view?.webview.postMessage({ type: "catalogue", catalogue: this.catalogue });
    }
  }

  private async open(file: string): Promise<void> {
    const root = workspace.workspaceFolders?.[0]?.uri;
    if (!root) {
      return;
    }
    const document = await workspace.openTextDocument(Uri.joinPath(root, file));
    await window.showTextDocument(document);
  }

  private html(webview: Webview): string {
    const asset = (name: string) =>
      webview.asWebviewUri(Uri.joinPath(this.extension, "media", name));
    // A webview inherits no origin, so without an explicit policy any string that
    // reaches the DOM is executable; the nonce keeps that to our own script.
    const nonce = randomBytes(16).toString("base64");
    // Twice the drawn size, so a HiDPI display has real pixels to show.
    const mascot = asset("mascot-128.png");
    return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta http-equiv="Content-Security-Policy"
      content="default-src 'none'; img-src ${webview.cspSource}; style-src ${webview.cspSource}; script-src 'nonce-${nonce}';" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<link rel="stylesheet" href="${asset("panel.css")}" />
<title>Rqx</title>
</head>
<body data-owl="${mascot}">
<div id="root"></div>
<script nonce="${nonce}" src="${asset("panel.js")}"></script>
</body>
</html>`;
  }
}
