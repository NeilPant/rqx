import { average } from "./middle";
import { total } from "./leaf";

// Each group is passed on as it is, so what it costs is what the group's own size costs.

// @complexity time=O(n*m) space=O(n) conf=verified
export function averages(groups: number[][]): number[] {
  const result: number[] = [];
  for (const group of groups) {
    result.push(average(group));
  }
  return result;
}

// The whole of the input is passed on once for every element of it.

// @complexity time=O(n^2) space=O(1) conf=verified
export function shares(values: number[]): number {
  let largest = 0;
  for (const value of values) {
    largest = Math.max(largest, value / total(values));
  }
  return largest;
}
