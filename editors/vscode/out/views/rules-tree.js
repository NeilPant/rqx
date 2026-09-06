"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.RulesTree = exports.RULES_VIEW = void 0;
const vscode_1 = require("vscode");
exports.RULES_VIEW = "rqx.rules";
/// The catalogue, with how often each rule fired here. A rule that never fires is
/// still worth seeing — it is what the project is being held to — so hiding those is
/// a choice rather than the default.
class RulesTree {
    index;
    changed = new vscode_1.EventEmitter();
    view;
    firedOnly = false;
    onDidChangeTreeData = this.changed.event;
    constructor(index) {
        this.index = index;
        index.onDidChange(() => this.refresh());
    }
    register() {
        this.view = vscode_1.window.createTreeView(exports.RULES_VIEW, { treeDataProvider: this });
        this.refresh();
        return this.view;
    }
    toggleFiredOnly() {
        this.firedOnly = !this.firedOnly;
        this.refresh();
    }
    refresh() {
        this.changed.fire(undefined);
        if (this.view) {
            this.view.description = this.firedOnly ? "fired here" : undefined;
            this.view.message =
                this.index.rules.length === 0 ? "The catalogue arrives with the first scan." : undefined;
        }
    }
    get families() {
        const fired = this.index.firedCounts();
        const kept = this.index.rules.filter((rule) => !this.firedOnly || fired.has(rule.slug));
        const families = new Map();
        for (const rule of kept) {
            const members = families.get(rule.family);
            if (members)
                members.push(rule);
            else
                families.set(rule.family, [rule]);
        }
        return [...families.entries()]
            .sort((left, right) => left[0].localeCompare(right[0]))
            .map(([family, rules]) => ({ kind: "family", family, rules }));
    }
    getTreeItem(node) {
        if (node.kind === "family") {
            const item = new vscode_1.TreeItem(node.family, vscode_1.TreeItemCollapsibleState.Collapsed);
            item.description = String(node.rules.length);
            item.contextValue = "rqx-family";
            return item;
        }
        const item = new vscode_1.TreeItem(node.rule.slug, vscode_1.TreeItemCollapsibleState.None);
        item.description = node.fired > 0 ? `${node.fired} here` : undefined;
        item.tooltip = `${node.rule.why}\n\nMeasured by ${node.rule.metric}\nFix — ${node.rule.fixes.join(" / ")}`;
        item.iconPath = new vscode_1.ThemeIcon(node.fired > 0 ? "circle-filled" : "circle-outline", new vscode_1.ThemeColor(node.fired > 0 ? "list.warningForeground" : "list.deemphasizedForeground"));
        item.contextValue = "rqx-rule";
        item.command = {
            command: "rqx.explainRule",
            title: "Explain",
            arguments: [node.rule.slug],
        };
        return item;
    }
    getChildren(node) {
        if (!node)
            return this.families;
        if (node.kind === "rule")
            return [];
        const fired = this.index.firedCounts();
        return node.rules
            .map((rule) => ({ kind: "rule", rule, fired: fired.get(rule.slug) ?? 0 }))
            .sort((left, right) => right.fired - left.fired || left.rule.slug.localeCompare(right.rule.slug));
    }
}
exports.RulesTree = RulesTree;
//# sourceMappingURL=rules-tree.js.map