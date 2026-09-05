import {
  Event,
  EventEmitter,
  ThemeColor,
  ThemeIcon,
  TreeDataProvider,
  TreeItem,
  TreeItemCollapsibleState,
  TreeView,
  window,
} from "vscode";

import { FindingsIndex, Rule } from "../findings-index";

export const RULES_VIEW = "rqx.rules";

interface FamilyNode {
  readonly kind: "family";
  readonly family: string;
  readonly rules: Rule[];
}

interface RuleNode {
  readonly kind: "rule";
  readonly rule: Rule;
  readonly fired: number;
}

export type RulesNode = FamilyNode | RuleNode;

/// The catalogue, with how often each rule fired here. A rule that never fires is
/// still worth seeing — it is what the project is being held to — so hiding those is
/// a choice rather than the default.
export class RulesTree implements TreeDataProvider<RulesNode> {
  private readonly changed = new EventEmitter<RulesNode | undefined>();
  private view: TreeView<RulesNode> | undefined;
  private firedOnly = false;

  public readonly onDidChangeTreeData: Event<RulesNode | undefined> = this.changed.event;

  constructor(private readonly index: FindingsIndex) {
    index.onDidChange(() => this.refresh());
  }

  public register(): TreeView<RulesNode> {
    this.view = window.createTreeView<RulesNode>(RULES_VIEW, { treeDataProvider: this });
    this.refresh();
    return this.view;
  }

  public toggleFiredOnly(): void {
    this.firedOnly = !this.firedOnly;
    this.refresh();
  }

  public refresh(): void {
    this.changed.fire(undefined);
    if (this.view) {
      this.view.description = this.firedOnly ? "fired here" : undefined;
      this.view.message =
        this.index.rules.length === 0 ? "The catalogue arrives with the first scan." : undefined;
    }
  }

  private get families(): FamilyNode[] {
    const fired = this.index.firedCounts();
    const kept = this.index.rules.filter((rule) => !this.firedOnly || fired.has(rule.slug));
    const families = new Map<string, Rule[]>();
    for (const rule of kept) {
      const members = families.get(rule.family);
      if (members) members.push(rule);
      else families.set(rule.family, [rule]);
    }
    return [...families.entries()]
      .sort((left, right) => left[0].localeCompare(right[0]))
      .map(([family, rules]) => ({ kind: "family", family, rules }));
  }

  public getTreeItem(node: RulesNode): TreeItem {
    if (node.kind === "family") {
      const item = new TreeItem(node.family, TreeItemCollapsibleState.Collapsed);
      item.description = String(node.rules.length);
      item.contextValue = "rqx-family";
      return item;
    }
    const item = new TreeItem(node.rule.slug, TreeItemCollapsibleState.None);
    item.description = node.fired > 0 ? `${node.fired} here` : undefined;
    item.tooltip = `${node.rule.why}\n\nMeasured by ${node.rule.metric}\nFix — ${node.rule.fixes.join(" / ")}`;
    item.iconPath = new ThemeIcon(
      node.fired > 0 ? "circle-filled" : "circle-outline",
      new ThemeColor(node.fired > 0 ? "list.warningForeground" : "list.deemphasizedForeground"),
    );
    item.contextValue = "rqx-rule";
    item.command = {
      command: "rqx.explainRule",
      title: "Explain",
      arguments: [node.rule.slug],
    };
    return item;
  }

  public getChildren(node?: RulesNode): RulesNode[] {
    if (!node) return this.families;
    if (node.kind === "rule") return [];
    const fired = this.index.firedCounts();
    return node.rules
      .map((rule) => ({ kind: "rule", rule, fired: fired.get(rule.slug) ?? 0 }) as RuleNode)
      .sort((left, right) => right.fired - left.fired || left.rule.slug.localeCompare(right.rule.slug));
  }
}
