export function tooManyParameters(a: number, b: number, c: number, d: number, e: number) {
  return a + b + c + d + e;
}

export function deeplyNested(rows: number[]) {
  for (const row of rows) {
    if (row > 0) {
      while (row < 10) {
        if (row === 5) {
          return row;
        }
      }
    }
  }
  return 0;
}

export class Overgrown {
  a1 = 1; a2 = 1; a3 = 1; a4 = 1; a5 = 1; a6 = 1; a7 = 1; a8 = 1; a9 = 1; a10 = 1;
  a11 = 1; a12 = 1; a13 = 1; a14 = 1; a15 = 1; a16 = 1; a17 = 1; a18 = 1; a19 = 1;
  m1() {} m2() {} m3() {}
}

export class SameAsFeeder extends Overgrown {}

export class Formatting {
  static pad(text: string): string { return text; }
  static trim(text: string): string { return text; }
}

export class Catalogue {
  private rows: number[];
  constructor(path: string) {
    this.rows = loadRows(path);
  }
  size(): number { return this.rows.length; }
  first(): number { return this.rows[0] ?? 0; }
}

function loadRows(path: string): number[] {
  return [path.length];
}

export function crowdedScope(seed: number): number {
  const c1 = seed; const c2 = seed; const c3 = seed; const c4 = seed;
  const c5 = seed; const c6 = seed; const c7 = seed; const c8 = seed;
  const c9 = seed; const c10 = seed; const c11 = seed; const c12 = seed;
  const c13 = seed; const c14 = seed; const c15 = seed; const c16 = seed;
  return [c1, c2, c3, c4, c5, c6, c7, c8, c9, c10, c11, c12, c13, c14, c15, c16].length;
}

export function widelyGuarded(seed: number): number {
  try {
    const d1 = seed; const d2 = seed; const d3 = seed; const d4 = seed;
    const d5 = seed; const d6 = seed; const d7 = seed; const d8 = seed;
    const d9 = seed; const d10 = seed; const d11 = seed;
    return [d1, d2, d3, d4, d5, d6, d7, d8, d9, d10, d11].length;
  } catch (error) {
    return 0;
  }
}

export function manyPaths(kind: string, rows: number[]): number {
  if (kind === "a") return 1;
  if (kind === "b") return 2;
  if (kind === "c") return 3;
  if (kind === "d") return 4;
  if (kind === "e") return 5;
  if (kind === "f") return 6;
  if (kind === "g") return 7;
  if (kind === "h") return 8;
  if (kind === "i") return 9;
  for (const row of rows) {
    if (row > 0 && row < 10) return row;
  }
  while (rows.length > 0) {
    if (rows.pop() === 0) break;
  }
  return 0;
}

export function spreadOut(rows: number[]): [number, number, number, number] {
  return [rows.length, rows.length, rows.length, rows.length];
}

export function loops(rows: number[]): number {
  return loops(rows);
}

export class Spiral {
  turn(rows: number[]): number {
    return this.turn(rows);
  }
}
