function totalLength(parts) {
  let total = 0;
  for (const part of parts) {
    total += part.length;
  }
  return total;
}

function shares(parts, weights) {
  const out = [];
  for (const weight of weights) {
    const whole = totalLength(parts);
    out.push(weight / whole);
  }
  return out;
}

function sharesOfEach(weights) {
  const out = [];
  for (const weight of weights) {
    const own = [String(weight)];
    out.push(weight / totalLength(own));
  }
  return out;
}
