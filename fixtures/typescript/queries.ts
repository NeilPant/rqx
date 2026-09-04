export class Ledger {
  private total = 0;
  private reads = 0;

  getTotal(): number {
    this.reads += 1;
    return this.total;
  }
}

export function handle(input: string): string {
  return input.trim();
}

export function summarise(rows: string[]): string {
  const separator = " | ";
  if (rows.length === 0) {
    return "";
  }
  const trimmed: string[] = [];
  for (const row of rows) {
    if (row.length === 0) {
      continue;
    }
    if (row.startsWith("#")) {
      continue;
    }
    trimmed.push(row.trim());
  }
  if (trimmed.length === 0) {
    return "";
  }
  if (trimmed.length === 1) {
    return trimmed[0];
  }
  return trimmed.join(separator);
}
