function totalLength(parts: string[]): number {
  let total = 0;
  for (const part of parts) {
    total += part.length;
  }
  return total;
}

function shares(parts: string[], weights: number[]): number[] {
  const out: number[] = [];
  for (const weight of weights) {
    const whole = totalLength(parts);
    out.push(weight / whole);
  }
  return out;
}

function sharesOfEach(weights: number[]): number[] {
  const out: number[] = [];
  for (const weight of weights) {
    const own = [String(weight)];
    out.push(weight / totalLength(own));
  }
  return out;
}
