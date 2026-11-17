function rankEach(groups, base) {
  const ranked = [];
  for (const group of groups) {
    ranked.push([...base].sort());
    ranked.push(group);
  }
  return ranked;
}

function rankOwn(groups) {
  const ranked = [];
  for (const group of groups) {
    ranked.push([...group].sort());
  }
  return ranked;
}
