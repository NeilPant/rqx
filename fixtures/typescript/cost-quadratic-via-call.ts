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

function describe(text: string): string {
  return text.trim().toUpperCase();
}

function describeEach(items: string[]): string[] {
  const out: string[] = [];
  for (const item of items) {
    out.push(describe(item));
  }
  return out;
}
