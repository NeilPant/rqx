// Code that looks expensive and is not: small fixed loops, keyed lookups, a memoised
// recursion, an exit on the first step. A rule that reports one of these is wrong.
// `scope=local` marks the functions whose own time the per-file extraction is held to.

// @complexity time=O(1) space=O(1) conf=verified scope=local
export function multiplicationTable() {
  let total = 0;
  for (let i = 0; i < 8; i++) {
    for (let j = 0; j < 8; j++) {
      total += i * j;
    }
  }
  return total;
}

// @complexity time=O(n) space=O(1) conf=verified scope=local
export function scaledByThreeFactors(values) {
  let total = 0;
  for (const factor of [1, 2, 3]) {
    for (const value of values) {
      total += value * factor;
    }
  }
  return total;
}

// @complexity time=O(n+m) space=O(m) conf=verified none=scan-in-loop,linear-op-in-loop
export function countKnown(values, known) {
  const lookup = new Set(known);
  let hits = 0;
  for (const value of values) {
    if (lookup.has(value)) {
      hits += 1;
    }
  }
  return hits;
}

// @complexity time=O(n) space=O(n) conf=verified none=branching-recursion
export function climbWays(steps, memo = new Map()) {
  if (steps <= 1) {
    return 1;
  }
  if (memo.has(steps)) {
    return memo.get(steps);
  }
  const ways = climbWays(steps - 1, memo) + climbWays(steps - 2, memo);
  memo.set(steps, ways);
  return ways;
}

// @complexity time=O(1) space=O(1) conf=verified
export function firstOrZero(values) {
  for (const value of values) {
    return value;
  }
  return 0;
}

// @complexity time=O(n) space=O(n) conf=verified none=linear-op-in-loop scope=local
export function tallyWords(words) {
  const counts = new Map();
  for (const word of words) {
    counts.set(word, (counts.get(word) ?? 0) + 1);
  }
  return counts;
}
