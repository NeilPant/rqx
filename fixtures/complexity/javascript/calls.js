// A function that calls another one, in the cases the cost rules tell apart.

// @complexity time=O(n) space=O(1) conf=verified scope=local
function indexOfItem(list, wanted) {
  for (let i = 0; i < list.length; i++) {
    if (list[i] === wanted) {
      return i;
    }
  }
  return -1;
}

// @complexity time=O(n^2) space=O(n) conf=verified rule=quadratic-via-call
export function firstOccurrences(items) {
  const firsts = [];
  for (let at = 0; at < items.length; at++) {
    if (indexOfItem(items, items[at]) === at) {
      firsts.push(items[at]);
    }
  }
  return firsts;
}

// @complexity time=O(1) space=O(1) conf=verified scope=local
function scale(x) {
  return x * 2;
}

// @complexity time=O(n) space=O(n) conf=verified none=quadratic-via-call
export function scaleEach(items) {
  const out = [];
  for (const item of items) {
    out.push(scale(item));
  }
  return out;
}

// @complexity time=O(n) space=O(1) conf=verified scope=local
function totalLength(parts) {
  let total = 0;
  for (const part of parts) {
    total += part.length;
  }
  return total;
}

// @complexity time=O(n*m) space=O(m) conf=verified rule=repeated-computation-in-loop
export function shares(parts, weights) {
  const out = [];
  for (const weight of weights) {
    const whole = totalLength(parts);
    out.push(weight / whole);
  }
  return out;
}

// @complexity time=O(n) space=O(n) conf=verified none=repeated-computation-in-loop
export function sharesOfEach(weights) {
  const out = [];
  for (const weight of weights) {
    out.push(weight / totalLength([String(weight)]));
  }
  return out;
}

// @complexity time=O(n) space=O(n) conf=verified rule=deep-recursion
export function sumTo(n) {
  if (n <= 0) {
    return 0;
  }
  return n + sumTo(n - 1);
}

// @complexity time=O(log n) space=O(log n) conf=verified none=deep-recursion
export function bits(n) {
  if (n <= 1) {
    return 0;
  }
  return 1 + bits(n >> 1);
}

// @complexity time=O(n^2) space=O(1) conf=verified scope=local none=pair-scan-could-hash
export function countPairs(items) {
  let pairs = 0;
  for (const left of items) {
    for (const right of items) {
      if (left === right) {
        pairs += 1;
      }
    }
  }
  return pairs;
}
