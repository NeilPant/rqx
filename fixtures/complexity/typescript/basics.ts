// Seed functions for the complexity precision corpus. Each annotation is the
// ground truth the inference is scored against; `space` counts the call stack.

// @complexity time=O(n) space=O(1) conf=verified
export function sumAll(values: number[]): number {
  let total = 0;
  for (const value of values) {
    total += value;
  }
  return total;
}

// @complexity time=O(n^2) space=O(1) conf=verified
export function hasDuplicate(values: number[]): boolean {
  for (let i = 0; i < values.length; i++) {
    for (let j = i + 1; j < values.length; j++) {
      if (values[i] === values[j]) {
        return true;
      }
    }
  }
  return false;
}

// @complexity time=O(n^2) space=O(n) conf=verified rule=linear-op-in-loop
export function dedupeWithIncludes(values: number[]): number[] {
  const unique: number[] = [];
  for (const value of values) {
    if (!unique.includes(value)) {
      unique.push(value);
    }
  }
  return unique;
}

// @complexity time=O(n) space=O(n) conf=verified none=linear-op-in-loop
export function dedupeWithSet(values: number[]): number[] {
  const seen = new Set<number>();
  const unique: number[] = [];
  for (const value of values) {
    if (!seen.has(value)) {
      seen.add(value);
      unique.push(value);
    }
  }
  return unique;
}

// @complexity time=O(n*m) space=O(1) conf=verified
export function countMatches(left: string[], right: string[]): number {
  let matches = 0;
  for (const a of left) {
    for (const b of right) {
      if (a === b) {
        matches += 1;
      }
    }
  }
  return matches;
}

// @complexity time=O(log n) space=O(1) conf=verified
export function binarySearch(sorted: number[], target: number): number {
  let low = 0;
  let high = sorted.length - 1;
  while (low <= high) {
    const mid = (low + high) >> 1;
    if (sorted[mid] === target) {
      return mid;
    }
    if (sorted[mid] < target) {
      low = mid + 1;
    } else {
      high = mid - 1;
    }
  }
  return -1;
}

// @complexity time=exp space=O(n) conf=verified rule=branching-recursion
export function fib(n: number): number {
  if (n < 2) {
    return n;
  }
  return fib(n - 1) + fib(n - 2);
}
