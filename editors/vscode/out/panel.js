"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.Panel = void 0;
const vscode_1 = require("vscode");
const node_crypto_1 = require("node:crypto");
const vscode_2 = require("vscode");
class Panel {
    extension;
    static id = "rqx.panel";
    view;
    report;
    constructor(extension) {
        this.extension = extension;
    }
    show(report) {
        this.report = report;
        this.view?.webview.postMessage({ type: "report", report: this.trimmed(report) });
    }
    /// The server ranks twenty; how many of them are worth a sidebar is the reader's call.
    trimmed(report) {
        const limit = vscode_2.workspace.getConfiguration("rqx").get("startHereLimit", 10);
        return { ...report, startHere: (report.startHere ?? []).slice(0, limit) };
    }
    reveal() {
        void vscode_1.commands.executeCommand(`${Panel.id}.focus`);
    }
    resolveWebviewView(view, _context, _token) {
        this.view = view;
        view.webview.options = {
            enableScripts: true,
            localResourceRoots: [vscode_1.Uri.joinPath(this.extension, "media")],
        };
        view.webview.html = this.html(view.webview);
        view.webview.onDidReceiveMessage((message) => {
            if (message.type === "ready") {
                this.replay();
            }
            else if (message.type === "open") {
                void this.open(message.file);
            }
            else if (message.type === "scan") {
                void vscode_1.commands.executeCommand("rqx.scanWorkspace");
            }
        });
    }
    replay() {
        if (this.report) {
            this.view?.webview.postMessage({ type: "report", report: this.trimmed(this.report) });
        }
    }
    async open(file) {
        const root = vscode_1.workspace.workspaceFolders?.[0]?.uri;
        if (!root) {
            return;
        }
        const document = await vscode_1.workspace.openTextDocument(vscode_1.Uri.joinPath(root, file));
        await vscode_1.window.showTextDocument(document);
    }
    html(webview) {
        const asset = (name) => webview.asWebviewUri(vscode_1.Uri.joinPath(this.extension, "media", name));
        // A webview inherits no origin, so without an explicit policy any string that
        // reaches the DOM is executable; the nonce keeps that to our own script.
        const nonce = (0, node_crypto_1.randomBytes)(16).toString("base64");
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
exports.Panel = Panel;
//# sourceMappingURL=panel.js.map