export class Ledger {
  total = 0;
  reads = 0;
  getTotal() {
    this.reads += 1;
    return this.total;
  }
}
export function handle(input) {
  return input.trim();
}
export function summarise(rows) {
  const separator = " | ";
  if (rows.length === 0) {
    return "";
  }
  const trimmed = [];
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
