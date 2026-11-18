// Code that resembles a finding and is not one. Every rule should stay silent here.

// output-parameter: `factor` in `counted` is a local that shares its name with
// this parameter, not this parameter.
export function scaled(factor) {
  return factor * 2;
}

export function counted(items) {
  let factor = items.length;
  factor += 1;
  return factor;
}
