// Functions copied unchanged from https://github.com/vitejs/vite at commit 60c260dea1fa9f628c22e32b14dddf6ca3088f2b.
// MIT. Copyright (c) 2019-present, VoidZero Inc. and Vite contributors.
// The license text is in ../NOTICE. Names a function uses that the project defines elsewhere are left undefined.

// From vitejs/vite packages/vite/src/node/utils.ts:1937 at 60c260dea1fa.
// @complexity time=O(n) space=O(1) conf=verified
export function arrayEqual(a: any[], b: any[]): boolean {
  if (a === b) return true
  if (a.length !== b.length) return false
  for (let i = 0; i < a.length; i++) {
    if (a[i] !== b[i]) return false
  }
  return true
}

// From vitejs/vite packages/vite/src/node/utils.ts:2026 at 60c260dea1fa.
// @complexity time=O(n log n) space=O(n) conf=verified
export function sortObjectKeys<T extends Record<string, any>>(obj: T): T {
  const sorted: Record<string, any> = {}
  for (const key of Object.keys(obj).sort()) {
    sorted[key] = obj[key]
  }
  return sorted as T
}

// From vitejs/vite packages/vite/src/node/utils.ts:1438 at 60c260dea1fa.
// @complexity time=O(n) space=O(n) conf=heuristic
export function deepClone<T>(value: T): DeepWritable<T> {
  if (Array.isArray(value)) {
    return value.map((v) => deepClone(v)) as DeepWritable<T>
  }
  if (isObject(value)) {
    const cloned: Record<string, any> = {}
    for (const key in value) {
      cloned[key] = deepClone(value[key])
    }
    return cloned as DeepWritable<T>
  }
  if (typeof value === 'function') {
    return value as DeepWritable<T>
  }
  if (value instanceof RegExp) {
    return new RegExp(value) as DeepWritable<T>
  }
  if (typeof value === 'object' && value != null) {
    throw new Error('Cannot deep clone non-plain object')
  }
  return value as DeepWritable<T>
}

// From vitejs/vite packages/vite/src/node/utils.ts:508 at 60c260dea1fa.
// @complexity time=O(n) space=O(n) conf=heuristic
export function posToNumber(source: string, pos: number | Pos): number {
  if (typeof pos === 'number') return pos
  const lines = source.split(splitRE)
  const { line, column } = pos
  let start = 0
  for (let i = 0; i < line - 1 && i < lines.length; i++) {
    start += lines[i].length
    start += lineTerminatorLengthAt(source, start)
  }
  return start + column
}

// From vitejs/vite packages/vite/src/node/utils.ts:1359 at 60c260dea1fa.
// @complexity time=O(n^2) space=O(n) conf=heuristic
export function extractHostnamesFromSubjectAltName(
  subjectAltName: string,
): string[] {
  const hostnames: string[] = []
  let remaining = subjectAltName
  while (remaining) {
    const nameEndIndex = remaining.indexOf(':')
    const name = remaining.slice(0, nameEndIndex)
    remaining = remaining.slice(nameEndIndex + 1)
    if (!remaining) break

    const isQuoted = remaining[0] === '"'
    let value: string
    if (isQuoted) {
      const endQuoteIndex = remaining.indexOf('"', 1)
      value = JSON.parse(remaining.slice(0, endQuoteIndex + 1))
      remaining = remaining.slice(endQuoteIndex + 1)
    } else {
      const maybeEndIndex = remaining.indexOf(',')
      const endIndex = maybeEndIndex === -1 ? remaining.length : maybeEndIndex
      value = remaining.slice(0, endIndex)
      remaining = remaining.slice(endIndex)
    }
    remaining = remaining.slice(/* for , */ 1).trimStart()

    if (
      name === 'DNS' &&
      // [::1] might be included but skip it as it's already included as a local address
      value !== '[::1]' &&
      // skip *.IPv4 addresses, which is invalid
      !(value.startsWith('*.') && net.isIPv4(value.slice(2)))
    ) {
      hostnames.push(value.replace('*', 'vite'))
    }
  }
  return hostnames
}
