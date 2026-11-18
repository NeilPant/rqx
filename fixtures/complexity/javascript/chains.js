// Three levels of calls in one file: the cost of the top is the loop it runs times what
// the levels below cost on each pass.

// @complexity time=O(n) space=O(1) conf=verified scope=local
function rowTotal(row) {
  let total = 0;
  for (const cell of row) {
    total += cell;
  }
  return total;
}

// @complexity time=O(n) space=O(1) conf=verified
function rowSummary(row) {
  return rowTotal(row) / Math.max(1, row.length);
}

// @complexity time=O(n*m) space=O(n) conf=verified
export function rowReport(rows) {
  const summaries = [];
  for (const row of rows) {
    summaries.push(rowSummary(row));
  }
  return summaries;
}

// @complexity time=O(n*m) space=O(1) conf=verified rule=repeated-computation-in-loop,quadratic-via-call
export function shareOfEach(rows, grand) {
  let largest = 0;
  for (const row of rows) {
    largest = Math.max(largest, rowSummary(grand) * row.length);
  }
  return largest;
}
