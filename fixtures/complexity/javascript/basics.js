// Seed functions for the complexity precision corpus. Each annotation is the
// ground truth the inference is scored against; `space` counts the call stack.

// @complexity time=O(n) space=O(1) conf=verified scope=local
export function sumAll(values) {
  let total = 0;
  for (const value of values) {
    total += value;
  }
  return total;
}

// @complexity time=O(1) space=O(1) conf=verified scope=local
export function firstThree(values) {
  const picked = [];
  for (let i = 0; i < 3; i++) {
    picked.push(values[i]);
  }
  return picked;
}

// @complexity time=O(n*m) space=O(1) conf=verified rule=quadratic-via-call
export function countShared(left, right) {
  let shared = 0;
  for (const item of left) {
    if (contains(right, item)) {
      shared += 1;
    }
  }
  return shared;
}

// @complexity time=O(n) space=O(1) conf=verified scope=local
function contains(items, wanted) {
  for (const item of items) {
    if (item === wanted) {
      return true;
    }
  }
  return false;
}

// @complexity time=O(n^2) space=O(n) conf=verified rule=accumulate-by-copy
export function collect(values) {
  let collected = [];
  for (const value of values) {
    collected = [...collected, value];
  }
  return collected;
}

// @complexity time=O(n) space=O(n) conf=verified none=branching-recursion
export function fibMemo(n, memo = new Map()) {
  if (n < 2) {
    return n;
  }
  if (memo.has(n)) {
    return memo.get(n);
  }
  const value = fibMemo(n - 1, memo) + fibMemo(n - 2, memo);
  memo.set(n, value);
  return value;
}
