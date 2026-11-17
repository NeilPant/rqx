// Loop bounds, one class each: a fixed number, the size of a collection, a count, a
// logarithm, and the shapes that must not change the answer. `scope=local` marks the
// functions whose own time the per-file extraction is held to.

const WINDOW = 8;

// @complexity time=O(1) space=O(1) conf=verified scope=local
export function windowSum(values: number[]): number {
  let total = 0;
  for (let i = 0; i < WINDOW; i++) {
    total += values[i] ?? 0;
  }
  return total;
}

// @complexity time=O(log n) space=O(1) conf=verified scope=local
export function halvings(n: number): number {
  let steps = 0;
  while (n > 1) {
    n = Math.floor(n / 2);
    steps += 1;
  }
  return steps;
}

// @complexity time=O(log n) space=O(1) conf=verified scope=local
export function powersBelow(limit: number): number {
  let count = 0;
  for (let p = 1; p < limit; p *= 2) {
    count += 1;
  }
  return count;
}

// @complexity time=O(n^2) space=O(1) conf=verified scope=local
export function countInversions(values: number[]): number {
  let inversions = 0;
  for (let i = 0; i < values.length; i++) {
    for (let j = i + 1; j < values.length; j++) {
      if (values[i] > values[j]) {
        inversions += 1;
      }
    }
  }
  return inversions;
}

// @complexity time=O(n) space=O(1) conf=verified scope=local
export function firstNegative(values: number[]): number | null {
  for (const value of values) {
    if (value < 0) {
      return value;
    }
  }
  return null;
}

// @complexity time=O(log n) space=O(1) conf=verified scope=local
export function lowerBound(sorted: number[], target: number): number {
  let low = 0;
  let high = sorted.length;
  while (low < high) {
    const mid = (low + high) >>> 1;
    if (sorted[mid] < target) {
      low = mid + 1;
    } else {
      high = mid;
    }
  }
  return low;
}

// @complexity time=O(n) space=O(n) conf=verified scope=local
export function copyThenWalk(values: number[]): number {
  const copy = values.slice(1);
  let total = 0;
  for (const value of copy) {
    total += value;
  }
  return total;
}

// @complexity time=O(n*m) space=O(1) conf=verified scope=local
export function everyPair(left: string[], right: string[]): number {
  let pairs = 0;
  left.forEach((a) => {
    right.forEach((b) => {
      pairs += a.length + b.length;
    });
  });
  return pairs;
}

// @complexity time=O(n) space=O(n) conf=verified scope=local
export function lengths(words: string[]): number[] {
  return words.filter((w) => w.length > 0).map((w) => w.length);
}

// @complexity time=O(1) space=O(1) conf=verified scope=local
export function threeLetters(): string {
  let joined = "";
  for (const letter of ["a", "b", "c"]) {
    joined += letter;
  }
  return joined;
}
