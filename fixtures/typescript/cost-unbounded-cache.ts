const pageSizes = new Map<string, number>();

function rememberPage(key: string, size: number): number {
  pageSizes.set(key, size);
  return size;
}

const recentSizes = new Map<string, number>();

function rememberRecentPage(key: string, size: number): number {
  recentSizes.set(key, size);
  if (recentSizes.size > 100) {
    recentSizes.clear();
  }
  return size;
}
