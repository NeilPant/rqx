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
