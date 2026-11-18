// Callbacks are loops: each one adds a level, and a callback that scans adds a scan. The
// receivers have no declared type. `scope=local` marks the functions whose own time the
// per-file extraction is held to.

// @complexity time=O(n) space=O(n) conf=verified scope=local
export function evenSquares(values) {
  return values.filter((v) => v % 2 === 0).map((v) => v * v);
}

// @complexity time=O(n*m) space=O(n*m) conf=verified scope=local
export function pairProducts(left, right) {
  return left.flatMap((a) => right.map((b) => a * b));
}

// @complexity time=O(n*m) space=O(1) conf=heuristic rule=scan-in-loop scope=local
export function anyListed(values, wanted) {
  return values.some((value) => wanted.includes(value));
}

// @complexity time=O(n) space=O(1) conf=verified scope=local
export function totalLength(words) {
  return words.reduce((sum, word) => sum + word.length, 0);
}

// @complexity time=O(n) space=O(1) conf=verified scope=local
export function firstLong(words, limit) {
  return words.find((word) => word.length > limit);
}
