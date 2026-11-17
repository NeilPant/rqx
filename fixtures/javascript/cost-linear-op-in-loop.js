function drainFromFront(items) {
  const entries = [...items];
  let total = 0;
  while (entries.length > 0) {
    total += (entries.shift() ?? 0);
  }
  return total;
}

function drainFromBack(items) {
  const leftovers = [...items];
  let total = 0;
  while (leftovers.length > 0) {
    total += (leftovers.pop() ?? 0);
  }
  return total;
}
