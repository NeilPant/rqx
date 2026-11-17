// Three files, each calling the one below it.

// @complexity time=O(n) space=O(1) conf=verified
export function total(values: number[]): number {
  let sum = 0;
  for (const value of values) {
    sum += value;
  }
  return sum;
}
