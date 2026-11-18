// Loop shapes in plain JavaScript: nesting, sequence, early exits, halving, and a
// search that throws half of what is left away on every step. `scope=local` marks the
// functions whose own time the per-file extraction is held to.

// @complexity time=O(n^2) space=O(1) conf=verified scope=local
export function closestGap(values) {
  let best = Infinity;
  for (let i = 0; i < values.length; i++) {
    for (let j = i + 1; j < values.length; j++) {
      best = Math.min(best, Math.abs(values[i] - values[j]));
    }
  }
  return best;
}

// @complexity time=O(n) space=O(1) conf=verified scope=local
export function sumThenScale(values) {
  let total = 0;
  for (const value of values) {
    total += value;
  }
  let scaled = 0;
  for (const value of values) {
    scaled += value * total;
  }
  return scaled;
}

// @complexity time=O(n^3) space=O(1) conf=verified scope=local
export function zeroTriples(values) {
  let hits = 0;
  for (let i = 0; i < values.length; i++) {
    for (let j = i + 1; j < values.length; j++) {
      for (let k = j + 1; k < values.length; k++) {
        if (values[i] + values[j] + values[k] === 0) {
          hits += 1;
        }
      }
    }
  }
  return hits;
}

// @complexity time=O(n*m) space=O(1) conf=verified scope=local
export function gridTotal(rows, width) {
  let total = 0;
  for (const row of rows) {
    for (let column = 0; column < width; column++) {
      total += row[column];
    }
  }
  return total;
}

// @complexity time=O(log n) space=O(1) conf=verified scope=local
export function bitLength(n) {
  let bits = 0;
  while (n > 0) {
    n = n >> 1;
    bits += 1;
  }
  return bits;
}

// @complexity time=O(log n) space=O(1) conf=verified scope=local
export function searchSorted(sorted, target) {
  let low = 0;
  let high = sorted.length - 1;
  while (low <= high) {
    const mid = Math.floor((low + high) / 2);
    if (sorted[mid] === target) {
      return mid;
    }
    if (sorted[mid] < target) {
      low = mid + 1;
    } else {
      high = mid - 1;
    }
  }
  return -1;
}

// @complexity time=O(n) space=O(1) conf=verified scope=local
export function firstRepeatOfNeighbour(values) {
  for (let i = 1; i < values.length; i++) {
    if (values[i] === values[i - 1]) {
      return i;
    }
  }
  return -1;
}

// @complexity time=O(n) space=O(n) conf=verified scope=local
export function runningTotals(values) {
  const totals = [];
  let sum = 0;
  for (const value of values) {
    sum += value;
    totals.push(sum);
  }
  return totals;
}
