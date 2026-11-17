async function loadEveryPage(ids) {
  return Promise.all(ids.map((id) => fetch(id)));
}

async function loadFewPages(ids, pLimit) {
  const limit = pLimit(4);
  return Promise.all(ids.map((id) => limit(() => fetch(id))));
}

async function loadTwoPages(first, second) {
  return Promise.all([fetch(first), fetch(second)]);
}
