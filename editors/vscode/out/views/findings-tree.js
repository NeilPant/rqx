"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.FindingsTree = exports.FINDINGS_VIEW = void 0;
const vscode_1 = require("vscode");
exports.FINDINGS_VIEW = "rqx.findings";
const SEVERITY_ICON = {
    error: { id: "error", colour: "list.errorForeground" },
    warning: { id: "warning", colour: "list.warningForeground" },
    info: { id: "info", colour: "list.deemphasizedForeground" },
};
const SEVERITY_RANK = ["error", "warning", "info"];
function keyOf(finding, grouping) {
    if (grouping === "rule")
        return finding.rule;
    if (grouping === "severity")
        return finding.severity;
    if (grouping === "directory") {
        const cut = finding.file.lastIndexOf("/");
        return cut === -1 ? "." : finding.file.slice(0, cut);
    }
    return finding.file;
}
/// Every finding the last scan published, grouped and narrowed the way the reader
/// asked. It reads the index rather than the Problems panel, so a client that has
/// been told to render everything quietly still groups by the severity rqx declared.
class FindingsTree {
    index;
    filters;
    changed = new vscode_1.EventEmitter();
    view;
    onDidChangeTreeData = this.changed.event;
    constructor(index, filters) {
        this.index = index;
        this.filters = filters;
        index.onDidChange(() => this.refresh());
        filters.onDidChange(() => this.refresh());
    }
    register() {
        this.view = vscode_1.window.createTreeView(exports.FINDINGS_VIEW, {
            treeDataProvider: this,
            showCollapseAll: true,
        });
        this.refresh();
        return this.view;
    }
    refresh() {
        this.changed.fire(undefined);
        this.describe();
    }
    get shown() {
        return this.index.findings
            .filter((finding) => this.filters.matches(finding))
            .sort((left, right) => SEVERITY_RANK.indexOf(left.severity) - SEVERITY_RANK.indexOf(right.severity) ||
            left.file.localeCompare(right.file) ||
            left.range.start.line - right.range.start.line);
    }
    /// The header carries the count and what is hiding the rest, because a filtered
    /// list that looks like the whole list is how somebody concludes a rule stopped
    /// firing.
    describe() {
        if (!this.view)
            return;
        const shown = this.shown.length;
        const total = this.index.findings.length;
        this.view.badge = shown > 0 ? { value: shown, tooltip: `${shown} findings` } : undefined;
        this.view.description = this.filters.active ? this.filters.summary : undefined;
        this.view.message =
            shown === 0 && total > 0
                ? `Every one of the ${total} findings is filtered out.`
                : undefined;
    }
    getTreeItem(node) {
        return node.kind === "group" ? groupItem(node) : leafItem(node.finding);
    }
    getChildren(node) {
        if (node?.kind === "finding")
            return [];
        if (node?.kind === "group") {
            return node.members.map((finding) => ({ kind: "finding", finding }));
        }
        const groups = new Map();
        for (const finding of this.shown) {
            const key = keyOf(finding, this.filters.value.grouping);
            const members = groups.get(key);
            if (members)
                members.push(finding);
            else
                groups.set(key, [finding]);
        }
        return [...groups.entries()]
            .sort((left, right) => right[1].length - left[1].length || left[0].localeCompare(right[0]))
            .map(([label, members]) => ({ kind: "group", label, members }));
    }
}
exports.FindingsTree = FindingsTree;
function groupItem(group) {
    const item = new vscode_1.TreeItem(group.label, vscode_1.TreeItemCollapsibleState.Expanded);
    item.description = String(group.members.length);
    item.contextValue = "rqx-group";
    item.tooltip = `${group.members.length} findings`;
    // Only a file group can be fixed or opened as a file, and the context value is
    // what decides which of those the menu offers.
    const file = group.members[0]?.file;
    if (file && group.members.every((member) => member.file === group.label)) {
        item.contextValue = "rqx-file";
        item.resourceUri = group.members[0]?.uri;
    }
    return item;
}
function leafItem(finding) {
    const item = new vscode_1.TreeItem(finding.rule, vscode_1.TreeItemCollapsibleState.None);
    const icon = SEVERITY_ICON[finding.severity];
    item.iconPath = new vscode_1.ThemeIcon(icon.id, new vscode_1.ThemeColor(icon.colour));
    item.description = finding.evidence;
    item.tooltip = [
        finding.evidence,
        finding.technique.length ? `Fix — ${finding.technique.join(" / ")}` : "",
        finding.confidence && finding.confidence !== "verified" ? `(${finding.confidence})` : "",
        finding.known ? "already in the baseline" : "",
    ]
        .filter(Boolean)
        .join("\n");
    item.contextValue = finding.autofixable ? "rqx-finding-fixable" : "rqx-finding";
    item.command = {
        command: "rqx.openFinding",
        title: "Open",
        arguments: [finding],
    };
    return item;
}
//# sourceMappingURL=findings-tree.js.map