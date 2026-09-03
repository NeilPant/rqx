export function endpoint(region: string, secure: boolean, retries: number): string {
  if (region.length > 0) {
    if (secure) {
      if (retries > 0) {
        return "https://api.example.com/v2/orders";
      }
    }
  }
  return "";
}

export function score(a: number, b: number, c: number, d: number): number {
  return (a + b) * (c - d) + Math.max(a, b) + Math.min(c, d) - (a * d);
}
