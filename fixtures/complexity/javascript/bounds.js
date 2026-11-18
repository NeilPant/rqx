// Loop bounds and library calls in plain JavaScript, where no declaration says what
// a receiver is. `scope=local` marks the functions whose own time the per-file
// extraction is held to.

// @complexity time=O(n) space=O(1) conf=verified scope=local
export function lastIndexOfMax(values) {
  let best = 0;
  for (let i = 0; i < values.length; i++) {
    if (values[i] >= values[best]) {
      best = i;
    }
  }
  return best;
}

// @complexity time=O(log n) space=O(1) conf=verified scope=local
export function digitsOf(n) {
  let digits = 0;
  while (n > 0) {
    n = Math.floor(n / 10);
    digits += 1;
  }
  return digits;
}

// @complexity time=O(n*m) space=O(1) conf=verified rule=pair-scan-could-hash scope=local
export function overlap(left, right) {
  let shared = 0;
  for (const a of left) {
    for (const b of right) {
      if (a === b) {
        shared += 1;
      }
    }
  }
  return shared;
}

// @complexity time=O(n*m) space=O(1) conf=heuristic rule=scan-in-loop scope=local
export function missing(wanted, available) {
  let gone = 0;
  for (const item of wanted) {
    if (!available.includes(item)) {
      gone += 1;
    }
  }
  return gone;
}

// @complexity time=O(n) space=O(1) conf=heuristic none=linear-op-in-loop scope=local
export function missingKeyed(wanted, availableSet) {
  let gone = 0;
  for (const item of wanted) {
    if (!availableSet.has(item)) {
      gone += 1;
    }
  }
  return gone;
}

// @complexity time=O(n) space=O(n) conf=verified scope=local
export function squares(values) {
  const out = [];
  for (const v of values) {
    out.push(v * v);
  }
  return out;
}

// @complexity time=O(n) space=O(n) conf=verified scope=local
export function doubled(values) {
  return values.map((v) => v * 2);
}
