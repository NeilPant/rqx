"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.FilesTree = exports.FILES_VIEW = void 0;
const vscode_1 = require("vscode");
const findings_index_1 = require("../findings-index");
exports.FILES_VIEW = "rqx.files";
/// Worst first, the same order `rqx.startHere` and the terminal's Start here use, so
/// an editor and the command line never send somebody to different files.
const GRADES = ["critical", "poor", "fair", "clean", "pristine"];
const GRADE_COLOUR = {
    critical: "list.errorForeground",
    poor: "list.errorForeground",
    fair: "list.warningForeground",
    clean: "list.deemphasizedForeground",
    pristine: "list.deemphasizedForeground",
};
class FilesTree {
    index;
    changed = new vscode_1.EventEmitter();
    view;
    sort = "grade";
    onDidChangeTreeData = this.changed.event;
    constructor(index) {
        this.index = index;
        index.onDidChange(() => this.refresh());
    }
    register() {
        this.view = vscode_1.window.createTreeView(exports.FILES_VIEW, { treeDataProvider: this });
        this.refresh();
        return this.view;
    }
    async pickSort() {
        const options = [
            { label: "Worst grade first", value: "grade" },
            { label: "Most findings first", value: "findings" },
            { label: "Most depended on first", value: "importers" },
            { label: "By path", value: "path" },
        ];
        const chosen = await vscode_1.window.showQuickPick(options.map((one) => ({
            ...one,
            label: `${one.value === this.sort ? "$(check) " : ""}${one.label}`,
        })), { title: "Order the files by" });
        if (chosen) {
            this.sort = chosen.value;
            this.refresh();
        }
    }
    refresh() {
        this.changed.fire(undefined);
        if (this.view) {
            const files = this.files.length;
            this.view.badge = files > 0 ? { value: files, tooltip: `${files} graded files` } : undefined;
            this.view.description = this.sort === "grade" ? undefined : this.sort;
        }
    }
    get files() {
        const report = this.index.report;
        if (!report)
            return [];
        const impact = report.impact ?? {};
        const nodes = Object.entries(report.grades ?? {}).map(([file, grade]) => ({
            kind: "file",
            file,
            grade,
            findings: this.index.findingsIn(file).length,
            importers: impact[file]?.importers ?? 0,
            commits: impact[file]?.commits ?? 0,
        }));
        const orders = {
            grade: (left, right) => GRADES.indexOf(left.grade) - GRADES.indexOf(right.grade) ||
                right.importers - left.importers ||
                left.file.localeCompare(right.file),
            findings: (left, right) => right.findings - left.findings,
            importers: (left, right) => right.importers - left.importers,
            path: (left, right) => left.file.localeCompare(right.file),
        };
        return nodes.sort(orders[this.sort]);
    }
    getTreeItem(node) {
        if (node.kind === "finding") {
            const item = new vscode_1.TreeItem(node.finding.rule, vscode_1.TreeItemCollapsibleState.None);
            item.description = node.finding.evidence;
            item.contextValue = node.finding.autofixable ? "rqx-finding-fixable" : "rqx-finding";
            item.command = { command: "rqx.openFinding", title: "Open", arguments: [node.finding] };
            return item;
        }
        const item = new vscode_1.TreeItem(node.file, vscode_1.TreeItemCollapsibleState.Collapsed);
        item.resourceUri = (0, findings_index_1.absolute)(node.file);
        item.iconPath = new vscode_1.ThemeIcon("circle-filled", new vscode_1.ThemeColor(GRADE_COLOUR[node.grade] ?? "list.deemphasizedForeground"));
        item.description = [
            node.grade,
            node.findings > 0 ? `${node.findings} findings` : "",
            node.importers > 0 ? `${node.importers} in` : "",
        ]
            .filter(Boolean)
            .join(" · ");
        item.tooltip = `${node.file}\n${node.grade} · ${node.findings} findings · ${node.importers} importers · ${node.commits} commits in 90 days`;
        item.contextValue = "rqx-file";
        return item;
    }
    getChildren(node) {
        if (!node)
            return this.files;
        if (node.kind === "finding")
            return [];
        return this.index
            .findingsIn(node.file)
            .map((finding) => ({ kind: "finding", finding }));
    }
}
exports.FilesTree = FilesTree;
//# sourceMappingURL=files-tree.js.map