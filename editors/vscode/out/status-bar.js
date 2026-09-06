"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.StatusBar = void 0;
const vscode_1 = require("vscode");
/// The health score where the editor can see it without being looked at, and the one
/// interruption worth making: the project got measurably worse, just now.
class StatusBar {
    onDrop;
    item;
    latest;
    /// The score the last notification was measured against, so a repository that stays
    /// bad is reported once rather than on every save.
    announced;
    constructor(onDrop) {
        this.onDrop = onDrop;
        this.item = vscode_1.window.createStatusBarItem(vscode_1.StatusBarAlignment.Right, 100);
        this.item.command = "rqx.openPanel";
    }
    get disposable() {
        return this.item;
    }
    get health() {
        return this.latest;
    }
    show(report) {
        const previous = this.latest?.health;
        this.latest = report;
        const mode = vscode_1.workspace.getConfiguration("rqx").get("statusBar", "health");
        if (mode === "off") {
            this.item.hide();
            return;
        }
        // The owl is the identity and stays put; the verdict moves to the background,
        // which is the one thing in a status bar that reads without being looked at.
        this.item.backgroundColor =
            report.health < 50
                ? new vscode_1.ThemeColor("statusBarItem.errorBackground")
                : report.health < 80
                    ? new vscode_1.ThemeColor("statusBarItem.warningBackground")
                    : undefined;
        // An arrow only where there is a previous score to compare against; a trend shown
        // on the first scan of a session would be invented.
        const drift = previous === undefined || previous === report.health
            ? ""
            : report.health > previous
                ? ` $(arrow-up)${report.health - previous}`
                : ` $(arrow-down)${previous - report.health}`;
        this.item.text =
            mode === "compact"
                ? `$(rqx-owl) ${report.health}`
                : `$(rqx-owl) rqx ${report.health}/100${drift}`;
        this.item.tooltip =
            `${report.findings} findings across ${report.files} files (${report.elapsedMs}ms)\n` +
                `Click to open the panel.`;
        this.item.show();
        this.announce(previous, report.health);
    }
    working(message) {
        this.item.text = `$(sync~spin) rqx ${message}`;
        this.item.show();
    }
    /// A spinner is cleared by whatever started it: waiting on the health notification
    /// leaves it turning forever when the command answers without one.
    settle(running) {
        if (this.latest) {
            this.show(this.latest);
        }
        else if (running) {
            this.item.text = "$(rqx-owl) rqx";
            this.item.show();
        }
    }
    stopped(text, tooltip) {
        this.item.text = `$(rqx-owl) ${text}`;
        this.item.tooltip = tooltip;
        this.item.show();
    }
    announce(previous, health) {
        const threshold = vscode_1.workspace.getConfiguration("rqx").get("notifyOnHealthDrop", 5);
        if (threshold <= 0 || previous === undefined) {
            return;
        }
        const drop = previous - health;
        if (drop < threshold || this.announced === health) {
            return;
        }
        this.announced = health;
        this.onDrop(drop, health);
    }
}
exports.StatusBar = StatusBar;
//# sourceMappingURL=status-bar.js.map