declare function readFileSync(path: string, encoding: string): string;

type Client = { get(id: number): string };
type Reader = { read(path: string): Promise<string> };

export function keepWanted(rows: number[], wanted: number[]): number[] {
  const kept: number[] = [];
  for (const row of rows) {
    if (wanted.indexOf(row) >= 0) {
      kept.push(row);
    }
  }
  return kept;
}

export function sumCube(grid: number[][][]): number {
  let total = 0;
  for (const plane of grid) {
    for (const line of plane) {
      for (const cell of line) {
        total += cell;
      }
    }
  }
  return total;
}

export function queryEach(client: Client, ids: number[]): string[] {
  const rows: string[] = [];
  for (const id of ids) {
    rows.push(client.get(id));
  }
  return rows;
}

export async function readEach(reader: Reader, paths: string[]): Promise<string[]> {
  const texts: string[] = [];
  for (const path of paths) {
    texts.push(await reader.read(path));
  }
  return texts;
}

export function countDigits(lines: string[]): number {
  let hits = 0;
  for (const line of lines) {
    const digits = new RegExp("^[0-9]+$");
    hits += digits.test(line) ? 1 : 0;
  }
  return hits;
}

export function flattenByCopy(groups: number[][]): number[] {
  let flat: number[] = [];
  for (const group of groups) {
    flat = [...flat, ...group];
  }
  return flat;
}

export function fibonacci(step: number): number {
  if (step < 2) {
    return step;
  }
  return fibonacci(step - 1) + fibonacci(step - 2);
}

export async function settingsOf(path: string): Promise<string> {
  return readFileSync(path, "utf8");
}

export function schedules(rows: number[]): Array<() => number> {
  const out: Array<() => number> = [];
  for (let at = 0; at < rows.length; at++) {
    out.push(function later(): number {
      return at;
    });
  }
  return out;
}

export function prunes(rows: number[]): number {
  for (const row of rows) {
    if (row < 0) {
      rows.splice(0, 1);
    }
  }
  return rows.length;
}

export async function bothWaits(): Promise<number> {
  const first = await loadOne();
  const second = await loadTwo();
  return first + second;
}

export function guards(value: number): number {
  globalThis.console.assert(value > 0);
  return value;
}

async function loadOne(): Promise<number> {
  return 1;
}

async function loadTwo(): Promise<number> {
  return 2;
}

let openTabs = 0;

export function tabsOpen(): number {
  return openTabs;
}

export async function paced(): Promise<number> {
  await sleep(500);
  return openTabs;
}

async function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => globalThis.setTimeout(resolve, ms));
}

export async function fires(rows: number[]): Promise<number> {
  loadOne();
  return rows.length;
}
