export function restated(ready: boolean): boolean {
  if (ready) {
    return true;
  } else {
    return false;
  }
}

export function compared(ready: boolean, count: number): number {
  if (ready === true) {
    return 1;
  }
  if (count === count) {
    return 2;
  }
  return 0;
}

export function sameEitherWay(count: number): number {
  if (count > 10) {
    return 99;
  } else {
    return 99;
  }
}

export function decidedNothing(count: number): void {
  if (count > 10) {
  }
}

export function buriedExits(a: number, b: number): number {
  const scale = a * b;
  for (const step of [1, 2, 3]) {
    if (step > scale) {
      return 1;
    }
    while (step > 0) {
      if (a > b) {
        return 3;
      }
      if (b > a) {
        return 4;
      }
      if (a === b) {
        return 5;
      }
      if (scale === 0) {
        return 6;
      }
    }
  }
  return 0;
}

export function assigns(a: number, b: number): number {
  let found = 0;
  if ((found = a + b)) {
    return found;
  }
  return 0;
}

export function wideSwitch(kind: string): number {
  switch (kind) {
    case "a":
      return 1;
    case "b":
      return 2;
    case "c":
      return 3;
    case "d":
      return 4;
    case "e":
      return 5;
    case "f":
      return 6;
    case "g":
      return 7;
    case "h":
      return 8;
    case "i":
      return 9;
    case "j":
      return 10;
    case "k":
      return 11;
    case "l":
      return 12;
    case "m":
      return 13;
    default:
      return 0;
  }
}

export function deeplyNestedCallbacks(): void {
  setTimeout(() => {
    setTimeout(() => {
      setTimeout(() => {
        setTimeout(() => {
          globalThis.console.log("deep");
        }, 1);
      }, 1);
    }, 1);
  }, 1);
}
