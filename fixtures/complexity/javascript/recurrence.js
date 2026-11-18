// Recurrences in plain JavaScript, one shape each: a halving that stops, a halving that
// scans, a call per half, a linear call that does a linear loop, and a walk that exits
// on a match. `space` counts the call stack.

// @complexity time=O(log n) space=O(log n) conf=verified
export function bitCount(n) {
  if (n <= 1) {
    return n;
  }
  return (n % 2) + bitCount(Math.floor(n / 2));
}

// @complexity time=O(n) space=O(log n) conf=verified
export function halvingScan(values, size) {
  let total = 0;
  for (let i = 0; i < size; i++) {
    total += values[i];
  }
  if (size <= 1) {
    return total;
  }
  return total + halvingScan(values, Math.floor(size / 2));
}

// @complexity time=O(n) space=O(log n) conf=verified none=branching-recursion
export function sumHalves(values, low, high) {
  if (high - low <= 1) {
    return values[low] ?? 0;
  }
  const mid = (low + high) >> 1;
  return sumHalves(values, low, mid) + sumHalves(values, mid, high);
}

// @complexity time=O(n^2) space=O(n) conf=verified rule=deep-recursion
export function triangleOfSums(n) {
  if (n <= 0) {
    return 0;
  }
  let inner = 0;
  for (let i = 0; i < n; i++) {
    inner += i;
  }
  return inner + triangleOfSums(n - 1);
}

// @complexity time=O(n) space=O(n) conf=verified rule=deep-recursion
export function indexFrom(values, wanted, at) {
  if (at >= values.length) {
    return -1;
  }
  if (values[at] === wanted) {
    return at;
  }
  return indexFrom(values, wanted, at + 1);
}
