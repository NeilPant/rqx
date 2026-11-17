// Two classes with a method of the same name, which a caller that does not know its
// receiver's class could be calling either of.

export class Scanner {
  // @complexity time=O(n) space=O(1) conf=verified
  run(items: number[]): number {
    let seen = 0;
    for (const item of items) {
      seen += item;
    }
    return seen;
  }
}

export class Comparer {
  // @complexity time=O(n^2) space=O(1) conf=verified
  run(items: number[]): number {
    let same = 0;
    for (const left of items) {
      for (const right of items) {
        if (left === right) {
          same += 1;
        }
      }
    }
    return same;
  }
}
