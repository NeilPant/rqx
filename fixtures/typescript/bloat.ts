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
