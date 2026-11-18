// A tour of the library cost tables for typed receivers. Every function loops over
// `values` and calls methods of one collection, so what the table says a call costs is
// what the loop costs per pass. `scope=local` marks the functions whose own time the
// per-file extraction is held to.

// @complexity time=O(n*m) space=O(1) conf=verified rule=scan-in-loop may=linear-op-in-loop scope=local
export function arraySearches(values: number[], items: number[]): number {
  let hits = 0;
  for (const value of values) {
    if (items.includes(value)) hits += 1;
    hits += items.indexOf(value);
    hits += items.lastIndexOf(value);
  }
  return hits;
}

// @complexity time=O(n*m) space=O(m) conf=verified rule=scan-in-loop,repeated-computation-in-loop none=accumulate-by-copy may=linear-op-in-loop scope=local
export function arrayCopies(values: number[], items: number[]): number {
  let size = 0;
  for (const value of values) {
    size += items.slice(value).length;
    size += items.concat([value]).length;
    size += items.join(",").length;
    size += items.toReversed().length;
  }
  return size;
}

// @complexity time=O(n*m log m) space=O(m) conf=verified rule=scan-in-loop,repeated-computation-in-loop may=linear-op-in-loop scope=local
export function arraySorts(values: number[], items: number[]): number {
  let size = 0;
  for (const value of values) {
    size += items.toSorted().length + value;
  }
  return size;
}

// @complexity time=O(n*m) space=O(1) conf=verified rule=linear-op-in-loop may=scan-in-loop scope=local
export function arrayMutations(values: number[], items: number[]): void {
  for (const value of values) {
    items.unshift(value);
    items.shift();
    items.splice(0, 1);
    items.fill(value);
    items.copyWithin(0, 1);
    items.reverse();
  }
}

// @complexity time=O(n) space=O(1) conf=verified none=linear-op-in-loop,scan-in-loop scope=local
export function arrayConstantOps(values: number[], items: number[]): number {
  let last = 0;
  for (const value of values) {
    items.push(value);
    last = items.at(-1) ?? 0;
    items.pop();
  }
  return last;
}

// @complexity time=O(n*m) space=O(m) conf=verified rule=linear-op-in-loop may=scan-in-loop scope=local
export function recordSnapshots(values: number[], record: Record<string, number>): number {
  let size = 0;
  for (const value of values) {
    size += Object.keys(record).length;
    size += Object.values(record).length;
    size += Object.entries(record).length;
    size += JSON.stringify(record).length;
    size += Object.assign({}, record).x ?? value;
  }
  return size;
}

// @complexity time=O(n*m) space=O(m) conf=verified scope=local
export function cloneEach(values: number[], record: Record<string, number>): number {
  let size = 0;
  for (const value of values) {
    size += structuredClone(record).x ?? value;
  }
  return size;
}

// @complexity time=O(n*m) space=O(m) conf=verified scope=local
export function parseEach(texts: string[]): number {
  let size = 0;
  for (const text of texts) {
    size += JSON.parse(text).length;
  }
  return size;
}

// @complexity time=O(n) space=O(n) conf=verified scope=local
export function copyOnce(values: number[]): number[] {
  return Array.from(values);
}

// @complexity time=O(n*m) space=O(1) conf=verified rule=scan-in-loop may=linear-op-in-loop scope=local
export function stringSearches(words: string[], text: string): number {
  let hits = 0;
  for (const word of words) {
    if (text.includes(word)) hits += 1;
    hits += text.indexOf(word);
    hits += text.lastIndexOf(word);
    hits += text.localeCompare(word);
    hits += text.match(word)?.length ?? 0;
  }
  return hits;
}

// @complexity time=O(n*m) space=O(m) conf=verified rule=scan-in-loop,repeated-computation-in-loop none=accumulate-by-copy may=linear-op-in-loop scope=local
export function stringCopies(words: string[], text: string): number {
  let size = 0;
  for (const word of words) {
    const copies = [
      text.slice(1),
      text.substring(1),
      text.split(word),
      text.replace(word, ""),
      text.replaceAll(word, ""),
      text.trim(),
      text.toLowerCase(),
      text.toUpperCase(),
      text.padStart(10),
      text.repeat(2),
    ];
    size += copies.length;
  }
  return size;
}

// @complexity time=O(n) space=O(1) conf=verified none=scan-in-loop,linear-op-in-loop scope=local
export function stringCharacters(positions: number[], text: string): string {
  let last = "";
  for (const position of positions) {
    last = text.charAt(position);
  }
  return last;
}

// @complexity time=O(n) space=O(1) conf=verified none=scan-in-loop,linear-op-in-loop scope=local
export function keyedOperations(values: number[], seen: Set<number>, names: Map<number, string>): number {
  let found = 0;
  for (const value of values) {
    seen.add(value);
    names.set(value, "x");
    if (seen.has(value) && names.get(value)) found += 1;
    seen.delete(value);
    names.delete(value);
  }
  return found;
}

// @complexity time=O(n*m) space=O(1) conf=verified
export function clearEach(groups: Set<number>[]): void {
  for (const group of groups) {
    group.clear();
  }
}

// @complexity time=O(n*m) space=O(1) conf=verified rule=scan-in-loop may=linear-op-in-loop scope=local
export function byteSearches(values: number[], bytes: Uint8Array): number {
  let hits = 0;
  for (const value of values) {
    hits += bytes.indexOf(value);
    if (bytes.includes(value)) hits += 1;
  }
  return hits;
}

// @complexity time=O(n*m) space=O(m) conf=verified rule=linear-op-in-loop none=repeated-computation-in-loop may=scan-in-loop scope=local
export function byteMutations(values: number[], bytes: Uint8Array): number {
  let size = 0;
  for (const value of values) {
    bytes.fill(value);
    size += bytes.slice(1).length;
  }
  return size;
}

// @complexity time=O(n*m log m) space=O(1) conf=verified rule=scan-in-loop may=linear-op-in-loop scope=local
export function byteSorts(rounds: number, bytes: Int32Array): void {
  for (let i = 0; i < rounds; i++) {
    bytes.sort();
  }
}

// @complexity time=O(n*m) space=O(m) conf=verified scope=local
export function chunkJoins(batches: Buffer[][]): number {
  let size = 0;
  for (const batch of batches) {
    size += Buffer.concat(batch).length;
  }
  return size;
}

// @complexity time=O(n*m) space=O(m) conf=verified scope=local
export function bufferTexts(values: number[], buffer: Buffer): number {
  let size = 0;
  for (const value of values) {
    size += buffer.toString().length + value;
  }
  return size;
}
