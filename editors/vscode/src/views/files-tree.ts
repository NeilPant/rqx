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

import { Finding, FindingsIndex, absolute } from "../findings-index";

export const FILES_VIEW = "rqx.files";

export type FileSort = "grade" | "findings" | "importers" | "path";

interface FileNode {
  readonly kind: "file";
  readonly file: string;
  readonly grade: string;
  readonly findings: number;
  readonly importers: number;
  readonly commits: number;
}

interface FindingNode {
  readonly kind: "finding";
  readonly finding: Finding;
}

export type FilesNode = FileNode | FindingNode;

/// Worst first, the same order `rqx.startHere` and the terminal's Start here use, so
/// an editor and the command line never send somebody to different files.
const GRADES = ["critical", "poor", "fair", "clean", "pristine"];

const GRADE_COLOUR: Record<string, string> = {
  critical: "list.errorForeground",
  poor: "list.errorForeground",
  fair: "list.warningForeground",
  clean: "list.deemphasizedForeground",
  pristine: "list.deemphasizedForeground",
};

export class FilesTree implements TreeDataProvider<FilesNode> {
  private readonly changed = new EventEmitter<FilesNode | undefined>();
  private view: TreeView<FilesNode> | undefined;
  private sort: FileSort = "grade";

  public readonly onDidChangeTreeData: Event<FilesNode | undefined> = this.changed.event;

  constructor(private readonly index: FindingsIndex) {
    index.onDidChange(() => this.refresh());
  }

  public register(): TreeView<FilesNode> {
    this.view = window.createTreeView<FilesNode>(FILES_VIEW, { treeDataProvider: this });
    this.refresh();
    return this.view;
  }

  public async pickSort(): Promise<void> {
    const options: { label: string; value: FileSort }[] = [
      { label: "Worst grade first", value: "grade" },
      { label: "Most findings first", value: "findings" },
      { label: "Most depended on first", value: "importers" },
      { label: "By path", value: "path" },
    ];
    const chosen = await window.showQuickPick(
      options.map((one) => ({
        ...one,
        label: `${one.value === this.sort ? "$(check) " : ""}${one.label}`,
      })),
      { title: "Order the files by" },
    );
    if (chosen) {
      this.sort = chosen.value;
      this.refresh();
    }
  }

  public refresh(): void {
    this.changed.fire(undefined);
    if (this.view) {
      const files = this.files.length;
      this.view.badge = files > 0 ? { value: files, tooltip: `${files} graded files` } : undefined;
      this.view.description = this.sort === "grade" ? undefined : this.sort;
    }
  }

  private get files(): FileNode[] {
    const report = this.index.report;
    if (!report) return [];
    const impact = report.impact ?? {};
    const nodes: FileNode[] = Object.entries(report.grades ?? {}).map(([file, grade]) => ({
      kind: "file",
      file,
      grade,
      findings: this.index.findingsIn(file).length,
      importers: impact[file]?.importers ?? 0,
      commits: impact[file]?.commits ?? 0,
    }));
    const orders: Record<FileSort, (left: FileNode, right: FileNode) => number> = {
      grade: (left, right) =>
        GRADES.indexOf(left.grade) - GRADES.indexOf(right.grade) ||
        right.importers - left.importers ||
        left.file.localeCompare(right.file),
      findings: (left, right) => right.findings - left.findings,
      importers: (left, right) => right.importers - left.importers,
      path: (left, right) => left.file.localeCompare(right.file),
    };
    return nodes.sort(orders[this.sort]);
  }

  public getTreeItem(node: FilesNode): TreeItem {
    if (node.kind === "finding") {
      const item = new TreeItem(node.finding.rule, TreeItemCollapsibleState.None);
      item.description = node.finding.evidence;
      item.contextValue = node.finding.autofixable ? "rqx-finding-fixable" : "rqx-finding";
      item.command = { command: "rqx.openFinding", title: "Open", arguments: [node.finding] };
      return item;
    }
    const item = new TreeItem(node.file, TreeItemCollapsibleState.Collapsed);
    item.resourceUri = absolute(node.file);
    item.iconPath = new ThemeIcon(
      "circle-filled",
      new ThemeColor(GRADE_COLOUR[node.grade] ?? "list.deemphasizedForeground"),
    );
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

  public getChildren(node?: FilesNode): FilesNode[] {
    if (!node) return this.files;
    if (node.kind === "finding") return [];
    return this.index
      .findingsIn(node.file)
      .map((finding) => ({ kind: "finding", finding }) as FindingNode);
  }
}
