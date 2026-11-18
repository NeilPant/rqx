// Library calls: a typed receiver is costed from the table at full confidence, a
// receiver whose type is not known only for the method names that mean one thing
// everywhere, and a keyed collection is a lookup. `scope=local` marks the functions
// whose own time the per-file extraction is held to.

// @complexity time=O(n*m) space=O(1) conf=verified rule=linear-op-in-loop scope=local
export function drain(queue: number[], count: number): void {
  for (let i = 0; i < count; i++) {
    queue.shift();
  }
}

// @complexity time=O(n*m log m) space=O(m) conf=heuristic none=scan-in-loop scope=local
export function sortEach(batches: number[][]): void {
  for (const batch of batches) {
    batch.sort();
  }
}

// @complexity time=O(n) space=O(1) conf=verified none=linear-op-in-loop scope=local
export function lookupAll(index: Map<string, number>, keys: string[]): number {
  let total = 0;
  for (const key of keys) {
    total += index.get(key) ?? 0;
  }
  return total;
}

// @complexity time=O(n*m) space=O(1) conf=verified rule=scan-in-loop scope=local
export function mentions(text: string, words: string[]): number {
  let hits = 0;
  for (const word of words) {
    if (text.includes(word)) {
      hits += 1;
    }
  }
  return hits;
}

// @complexity time=O(n) space=O(1) conf=heuristic none=linear-op-in-loop scope=local
export function seenBefore(values: number[], seenIds): number {
  let repeats = 0;
  for (const value of values) {
    if (seenIds.has(value)) {
      repeats += 1;
    }
  }
  return repeats;
}

// @complexity time=O(n log n) space=O(n) conf=verified scope=local
export function sortedCopy(values: number[]): number[] {
  return values.toSorted();
}

// @complexity time=O(n) space=O(n) conf=verified scope=local
export function keysOf(record: Record<string, number>): string[] {
  return Object.keys(record);
}

// @complexity time=O(n) space=O(n) conf=verified scope=local
export function encodeAll(rows: string[]): string {
  return JSON.stringify(rows);
}

// @complexity time=O(n) space=O(n) conf=verified scope=local
export function buildIndex(words: string[]): Map<string, number> {
  const index = new Map<string, number>();
  for (const word of words) {
    index.set(word, word.length);
  }
  return index;
}
