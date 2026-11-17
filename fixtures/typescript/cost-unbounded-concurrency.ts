declare function pLimit(count: number): <T>(work: () => Promise<T>) => Promise<T>;

async function loadEveryPage(ids: string[]): Promise<unknown[]> {
  return Promise.all(ids.map((id) => fetch(id)));
}

async function loadFewPages(ids: string[]): Promise<unknown[]> {
  const limit = pLimit(4);
  return Promise.all(ids.map((id) => limit(() => fetch(id))));
}

async function loadTwoPages(first: string, second: string): Promise<unknown[]> {
  return Promise.all([fetch(first), fetch(second)]);
}
