import { total } from "./leaf";

// @complexity time=O(n) space=O(1) conf=verified
export function average(values: number[]): number {
  return values.length === 0 ? 0 : total(values) / values.length;
}
