function describeRows(rows: number[]): number {
  let total = 0;
  for (const row of rows) {
    const pair = [row, row + 1];
    const label = { row, next: row + 1 };
    const spare = [row];
    total += pair.length + spare.length + label.next;
  }
  return total;
}

function bufferEachRow(rows: number[]): number {
  let total = 0;
  for (const row of rows) {
    const buffer = new Array(65536).fill(row);
    total += buffer.length;
  }
  return total;
}

function pairEachRow(rows: number[]): number {
  let total = 0;
  for (const row of rows) {
    const pair = [row, row + 1];
    total += pair.length;
  }
  return total;
}
