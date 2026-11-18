// Code that resembles a finding and is not one. Every rule should stay silent here.

// output-parameter: `factor` in `counted` is a local that shares its name with
// this parameter, not this parameter.
export function scaled(factor: number): number {
  return factor * 2;
}

export function counted(items: string[]): number {
  let factor = items.length;
  factor += 1;
  return factor;
}

// ambient-dependency: the clock and the random source are parameters, and the
// ambient ones are only their defaults.
export function delay(
  base: number,
  random: () => number = Math.random,
  clock = () => Date.now(),
): number {
  return base * random() + clock();
}

// mixed-language-file: an error message that names an update is not SQL.
export function updateFailure(id: string): string {
  return "Failed to update user " + id;
}
