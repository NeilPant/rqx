// Functions copied unchanged from https://github.com/vitejs/vite at commit 60c260dea1fa9f628c22e32b14dddf6ca3088f2b.
// MIT. Copyright (c) 2019-present, VoidZero Inc. and Vite contributors.
// The license text is in ../NOTICE. Names a function uses that the project defines elsewhere are left undefined. lookupTabWithUrl is from a file that credits Facebook, Inc. (MIT, 2015-present).

// From vitejs/vite packages/vite/bin/openChrome.js:55 at 60c260dea1fa.
// @complexity time=O(n*m*k) space=O(m) conf=heuristic none=scan-in-loop
function lookupTabWithUrl(lookupUrl, app) {
  const windows = app.windows()
  for (const window of windows) {
    for (const [tabIndex, tab] of window.tabs().entries()) {
      if (tab.url().includes(lookupUrl)) {
        return {
          targetTab: tab,
          targetTabIndex: tabIndex + 1,
          targetWindow: window,
        }
      }
    }
  }
}

// From vitejs/vite playground/optimize-deps/dep-lodash/cloneDeep.js:4 at 60c260dea1fa.
// @complexity time=O(n) space=O(n) conf=heuristic
function cloneDeep(value) {
  if (value == null || typeof value !== 'object') {
    return value
  }

  if (Array.isArray(value)) {
    return value.map(cloneDeep)
  }

  const cloned = {}
  for (const key in value) {
    if (Object.hasOwn(value, key)) {
      cloned[key] = cloneDeep(value[key])
    }
  }
  return cloned
}

// From vitejs/vite playground/legacy/main.js:39 at 60c260dea1fa.
// @complexity time=O(1) space=O(1) conf=verified
async function* asyncGenerator() {
  for (let i = 0; i < 3; i++) {
    await new Promise((resolve) => setTimeout(resolve, 10))
    yield i
  }
}

// From vitejs/vite playground/ssr/src/app.js:22 at 60c260dea1fa.
// @complexity time=O(1) space=O(1) conf=verified
async function renderRoot(rootDir) {
  const paths = Object.keys(pathRenderers).filter((key) => key !== '/')
  return `
    <ul>
      ${paths
        .map(
          (path) =>
            `<li><a href="${escapeHtml(path)}">${escapeHtml(path)}</a></li>`,
        )
        .join('\n')}
    </ul>
  `
}

// From vitejs/vite playground/legacy/immutable-chunk.js:11 at 60c260dea1fa.
// @complexity time=O(1) space=O(1) conf=verified
export function fn() {
  return Promise.all(
    chunks.map(async (name) => {
      const response = await fetch(`/assets/${name}.js`)
      return `${name}: ${response.headers.get('Content-Type')}`
    }),
  )
}
