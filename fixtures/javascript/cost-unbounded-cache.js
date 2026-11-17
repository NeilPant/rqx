const pageSizes = new Map();

function rememberPage(key, size) {
  pageSizes.set(key, size);
  return size;
}

const recentSizes = new Map();

function rememberRecentPage(key, size) {
  recentSizes.set(key, size);
  if (recentSizes.size > 100) {
    recentSizes.clear();
  }
  return size;
}
