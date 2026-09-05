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

import { Filters, Grouping } from "../filters";
import { Finding, FindingsIndex, Severity } from "../findings-index";

export const FINDINGS_VIEW = "rqx.findings";

interface Group {
  readonly kind: "group";
  readonly label: string;
  readonly members: Finding[];
}

interface Leaf {
  readonly kind: "finding";
  readonly finding: Finding;
}

export type FindingNode = Group | Leaf;

const SEVERITY_ICON: Record<Severity, { id: string; colour: string }> = {
  error: { id: "error", colour: "list.errorForeground" },
  warning: { id: "warning", colour: "list.warningForeground" },
  info: { id: "info", colour: "list.deemphasizedForeground" },
};

const SEVERITY_RANK: Severity[] = ["error", "warning", "info"];

function keyOf(finding: Finding, grouping: Grouping): string {
  if (grouping === "rule") return finding.rule;
  if (grouping === "severity") return finding.severity;
  if (grouping === "directory") {
    const cut = finding.file.lastIndexOf("/");
    return cut === -1 ? "." : finding.file.slice(0, cut);
  }
  return finding.file;
}

/// Every finding the last scan published, grouped and narrowed the way the reader
/// asked. It reads the index rather than the Problems panel, so a client that has
/// been told to render everything quietly still groups by the severity rqx declared.
export class FindingsTree implements TreeDataProvider<FindingNode> {
  private readonly changed = new EventEmitter<FindingNode | undefined>();
  private view: TreeView<FindingNode> | undefined;

  public readonly onDidChangeTreeData: Event<FindingNode | undefined> = this.changed.event;

  constructor(
    private readonly index: FindingsIndex,
    private readonly filters: Filters,
  ) {
    index.onDidChange(() => this.refresh());
    filters.onDidChange(() => this.refresh());
  }

  public register(): TreeView<FindingNode> {
    this.view = window.createTreeView<FindingNode>(FINDINGS_VIEW, {
      treeDataProvider: this,
      showCollapseAll: true,
    });
    this.refresh();
    return this.view;
  }

  public refresh(): void {
    this.changed.fire(undefined);
    this.describe();
  }

  private get shown(): Finding[] {
    return this.index.findings
      .filter((finding) => this.filters.matches(finding))
      .sort(
        (left, right) =>
          SEVERITY_RANK.indexOf(left.severity) - SEVERITY_RANK.indexOf(right.severity) ||
          left.file.localeCompare(right.file) ||
          left.range.start.line - right.range.start.line,
      );
  }

  /// The header carries the count and what is hiding the rest, because a filtered
  /// list that looks like the whole list is how somebody concludes a rule stopped
  /// firing.
  private describe(): void {
    if (!this.view) return;
    const shown = this.shown.length;
    const total = this.index.findings.length;
    this.view.badge = shown > 0 ? { value: shown, tooltip: `${shown} findings` } : undefined;
    this.view.description = this.filters.active ? this.filters.summary : undefined;
    this.view.message =
      shown === 0 && total > 0
        ? `Every one of the ${total} findings is filtered out.`
        : undefined;
  }

  public getTreeItem(node: FindingNode): TreeItem {
    return node.kind === "group" ? groupItem(node) : leafItem(node.finding);
  }

  public getChildren(node?: FindingNode): FindingNode[] {
    if (node?.kind === "finding") return [];
    if (node?.kind === "group") {
      return node.members.map((finding) => ({ kind: "finding", finding }));
    }
    const groups = new Map<string, Finding[]>();
    for (const finding of this.shown) {
      const key = keyOf(finding, this.filters.value.grouping);
      const members = groups.get(key);
      if (members) members.push(finding);
      else groups.set(key, [finding]);
    }
    return [...groups.entries()]
      .sort((left, right) => right[1].length - left[1].length || left[0].localeCompare(right[0]))
      .map(([label, members]) => ({ kind: "group", label, members }));
  }
}

function groupItem(group: Group): TreeItem {
  const item = new TreeItem(group.label, TreeItemCollapsibleState.Expanded);
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

function leafItem(finding: Finding): TreeItem {
  const item = new TreeItem(finding.rule, TreeItemCollapsibleState.None);
  const icon = SEVERITY_ICON[finding.severity];
  item.iconPath = new ThemeIcon(icon.id, new ThemeColor(icon.colour));
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
