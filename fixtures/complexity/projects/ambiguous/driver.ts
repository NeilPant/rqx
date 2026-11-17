// The receiver is untyped, so the call is taken to reach the costliest `run` there is.

// @complexity time=O(n*m^2) space=O(1) conf=heuristic
export function driveAll(workers: any[], items: number[]): number {
  let total = 0;
  for (const worker of workers) {
    total += worker.run(items);
  }
  return total;
}
