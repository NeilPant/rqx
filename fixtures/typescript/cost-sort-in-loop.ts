function rankEach(groups: number[][], base: number[]): number[][] {
  const ranked: number[][] = [];
  for (const group of groups) {
    ranked.push([...base].sort());
    ranked.push(group);
  }
  return ranked;
}

function rankOwn(groups: number[][]): number[][] {
  const ranked: number[][] = [];
  for (const group of groups) {
    ranked.push([...group].sort());
  }
  return ranked;
}
