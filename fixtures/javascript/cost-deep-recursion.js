function sumTo(n) {
  if (n <= 0) {
    return 0;
  }
  return n + sumTo(n - 1);
}

function bits(n) {
  if (n <= 1) {
    return 0;
  }
  return 1 + bits(n >> 1);
}
