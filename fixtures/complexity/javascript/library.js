// Library calls in plain JavaScript, where a parameter has no declared type. A method
// name that means one thing on every type is costed at lower confidence, and an array
// built in the function is known to be one. `scope=local` marks the functions whose own
// time the per-file extraction is held to.

// @complexity time=O(n*m) space=O(n) conf=heuristic rule=scan-in-loop scope=local
export function positionsOf(values, haystack) {
  const found = [];
  for (const value of values) {
    found.push(haystack.indexOf(value));
  }
  return found;
}

// @complexity time=O(n*m) space=O(n*m) conf=heuristic scope=local
export function joinEach(rows, glue) {
  const lines = [];
  for (const row of rows) {
    lines.push(row.join(glue));
  }
  return lines;
}

// @complexity time=O(n^2) space=O(1) conf=heuristic
export function drainQueue(queue) {
  let total = 0;
  while (queue.length > 0) {
    total += queue.shift();
  }
  return total;
}

// @complexity time=O(n*m) space=O(1) conf=heuristic scope=local
export function reverseEach(rows) {
  for (const row of rows) {
    row.reverse();
  }
}

// @complexity time=O(n*m) space=O(n*m) conf=heuristic none=scan-in-loop scope=local
export function flattenEach(groups) {
  const flat = [];
  for (const group of groups) {
    flat.push(group.flat());
  }
  return flat;
}

// @complexity time=O(n^2*m) space=O(n*m) conf=heuristic rule=accumulate-by-copy
export function concatAll(parts) {
  let all = [];
  for (const part of parts) {
    all = all.concat(part);
  }
  return all;
}

// @complexity time=O(n^2) space=O(n) conf=verified rule=linear-op-in-loop scope=local
export function reversedByInsertion(values) {
  const out = [];
  for (const value of values) {
    out.unshift(value);
  }
  return out;
}

// @complexity time=O(n) space=O(n) conf=verified none=linear-op-in-loop,scan-in-loop scope=local
export function keyedChurn(values) {
  const seen = new Set();
  const names = new Map();
  for (const value of values) {
    seen.add(value);
    names.set(value, seen.has(value));
    names.get(value);
  }
  return seen.size;
}
