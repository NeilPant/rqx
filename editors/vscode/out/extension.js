"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.activate = activate;
exports.deactivate = deactivate;
const vscode_1 = require("vscode");
const dashboard_1 = require("./dashboard");
const filters_1 = require("./filters");
const findings_index_1 = require("./findings-index");
const language_server_1 = require("./language-server");
const panel_1 = require("./panel");
const status_bar_1 = require("./status-bar");
const commands_1 = require("./commands");
const files_tree_1 = require("./views/files-tree");
const findings_tree_1 = require("./views/findings-tree");
const rules_tree_1 = require("./views/rules-tree");
let server;
async function activate(context) {
    const log = vscode_1.window.createOutputChannel("Rqx");
    const index = new findings_index_1.FindingsIndex();
    const filters = new filters_1.Filters(context);
    const panel = new panel_1.Panel(context.extensionUri);
    const findings = new findings_tree_1.FindingsTree(index, filters);
    const files = new files_tree_1.FilesTree(index);
    const rules = new rules_tree_1.RulesTree(index);
    const status = new status_bar_1.StatusBar((drop, health) => announce(drop, health, panel));
    const connection = new language_server_1.LanguageServer(context, log, index, {
        onHealth: (health) => status.show(health),
        onReport: (report) => {
            panel.show(report);
            // The welcome view offers a scan until there is something to show instead.
            void vscode_1.commands.executeCommand("setContext", "rqx.scanned", true);
        },
        onStateChange: () => status.settle(connection.running),
    });
    server = connection;
    const dashboard = new dashboard_1.Dashboard((command) => connection.send(command));
    context.subscriptions.push(status.disposable, log, vscode_1.window.registerWebviewViewProvider(panel_1.Panel.id, panel), findings.register(), files.register(), rules.register(), ...(0, commands_1.register)({
        server: connection,
        index,
        filters,
        panel,
        dashboard,
        findings,
        files,
        rules,
        scanning: (message) => status.working(message),
        settle: () => status.settle(connection.running),
        showLog: () => log.show(),
    }), vscode_1.workspace.onDidChangeConfiguration((event) => {
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
            void vscode_1.commands.executeCommand("rqx.scanWorkspace");
        }
        if (event.affectsConfiguration("rqx.startHereLimit") && index.report) {
            panel.show(index.report);
        }
        if (event.affectsConfiguration("rqx.findings")) {
            findings.refresh();
        }
    }));
    await connection.start();
}
async function reload(status, connection, reason) {
    status.working("reloading");
    try {
        await connection.restart(reason);
    }
    finally {
        status.settle(connection.running);
    }
}
function announce(drop, health, panel) {
    void vscode_1.window
        .showWarningMessage(`Rqx: health fell ${drop} points, to ${health}/100.`, "Open Panel", "Review the Diff")
        .then((choice) => {
        if (choice === "Open Panel") {
            panel.reveal();
        }
        else if (choice === "Review the Diff") {
            void vscode_1.commands.executeCommand("rqx.check");
        }
    });
}
function deactivate() {
    return server?.stop();
}
//# sourceMappingURL=extension.js.map