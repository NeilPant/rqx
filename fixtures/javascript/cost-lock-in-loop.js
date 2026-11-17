function tallyUnderOneLatch(latch, rows) {
  let total = 0;
  for (const row of rows) {
    latch.lock();
    total += row;
    latch.unlock();
  }
  return total;
}

function tallyUnderEachLatch(latches, rows) {
  let total = 0;
  for (let at = 0; at < rows.length; at++) {
    latches[at].lock();
    total += rows[at];
    latches[at].unlock();
  }
  return total;
}
