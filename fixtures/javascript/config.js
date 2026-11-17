export function endpoint(region, secure, retries) {
  if (region.length > 0) {
    if (secure) {
      if (retries > 0) {
        return "https://api.example.com/v2/orders";
      }
    }
  }
  return "";
}
export function score(a, b, c, d) {
  return (a + b) * (c - d) + Math.max(a, b) + Math.min(c, d) - (a * d);
}
