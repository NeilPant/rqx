function indexOfItem(list, wanted) {
  for (let i = 0; i < list.length; i++) {
    if (list[i] === wanted) {
      return i;
    }
  }
  return -1;
}

function firstOccurrences(items) {
  const firsts = [];
  for (let at = 0; at < items.length; at++) {
    if (indexOfItem(items, items[at]) === at) {
      firsts.push(items[at]);
    }
  }
  return firsts;
}

function missingFrom(items, known) {
  const missing = [];
  for (const item of items) {
    if (indexOfItem(known, item) < 0) {
      missing.push(item);
    }
  }
  return missing;
}
