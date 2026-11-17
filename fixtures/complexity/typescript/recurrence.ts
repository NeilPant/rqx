// Recursion, one shape each. Their cost needs the call graph, so none is marked local.

// @complexity time=O(n) space=O(n) conf=verified
export function factorial(n: number): number {
  return n <= 1 ? 1 : n * factorial(n - 1);
}

// @complexity time=O(log n) space=O(log n) conf=verified
export function power(base: number, exponent: number): number {
  if (exponent === 0) {
    return 1;
  }
  const half = power(base, Math.floor(exponent / 2));
  return exponent % 2 === 0 ? half * half : half * half * base;
}

// @complexity time=O(n log n) space=O(n) conf=verified
export function mergeSort(values: number[]): number[] {
  if (values.length <= 1) {
    return values;
  }
  const mid = values.length >> 1;
  const left = mergeSort(values.slice(0, mid));
  const right = mergeSort(values.slice(mid));
  const merged: number[] = [];
  let i = 0;
  let j = 0;
  while (i < left.length && j < right.length) {
    merged.push(left[i] <= right[j] ? left[i++] : right[j++]);
  }
  return merged.concat(left.slice(i), right.slice(j));
}

// @complexity time=O(n^2) space=O(n^2) conf=verified rule=accumulate-by-copy
export function sumTail(values: number[]): number {
  if (values.length === 0) {
    return 0;
  }
  return values[0] + sumTail(values.slice(1));
}

// @complexity time=exp space=O(n) conf=verified rule=branching-recursion
export function paths(n: number): number {
  if (n <= 1) {
    return 1;
  }
  return paths(n - 1) + paths(n - 2) + paths(n - 3);
}

// @complexity time=O(n) space=O(n) conf=verified none=branching-recursion
export function pathsMemo(n: number, memo: Map<number, number> = new Map()): number {
  if (n <= 1) {
    return 1;
  }
  const cached = memo.get(n);
  if (cached !== undefined) {
    return cached;
  }
  const value = pathsMemo(n - 1, memo) + pathsMemo(n - 2, memo);
  memo.set(n, value);
  return value;
}
