function indexOfItem(list: string[], wanted: string): number {
  for (let i = 0; i < list.length; i++) {
    if (list[i] === wanted) {
      return i;
    }
  }
  return -1;
}

function firstOccurrences(items: string[]): string[] {
  const firsts: string[] = [];
  for (let at = 0; at < items.length; at++) {
    if (indexOfItem(items, items[at]) === at) {
      firsts.push(items[at]);
    }
  }
  return firsts;
}

function missingFrom(items: string[], known: string[]): string[] {
  const missing: string[] = [];
  for (const item of items) {
    if (indexOfItem(known, item) < 0) {
      missing.push(item);
    }
  }
  return missing;
}
