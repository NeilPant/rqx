export function keepWanted(rows, wanted) {
  const kept = [];
  for (const row of rows) {
    if (wanted.indexOf(row) >= 0) {
      kept.push(row);
    }
  }
  return kept;
}
export function sumCube(grid) {
  let total = 0;
  for (const plane of grid) {
    for (const line of plane) {
      for (const cell of line) {
        total += cell;
      }
    }
  }
  return total;
}
export function queryEach(client, ids) {
  const rows = [];
  for (const id of ids) {
    rows.push(client.get(id));
  }
  return rows;
}
export async function readEach(reader, paths) {
  const texts = [];
  for (const path of paths) {
    texts.push(await reader.read(path));
  }
  return texts;
}
export function countDigits(lines) {
  let hits = 0;
  for (const line of lines) {
    const digits = new RegExp("^[0-9]+$");
    hits += digits.test(line) ? 1 : 0;
  }
  return hits;
}
export function flattenByCopy(groups) {
  let flat = [];
  for (const group of groups) {
    flat = [...flat, ...group];
  }
  return flat;
}
export function fibonacci(step) {
  if (step < 2) {
    return step;
  }
  return fibonacci(step - 1) + fibonacci(step - 2);
}
export async function settingsOf(path) {
  return readFileSync(path, "utf8");
}
export function schedules(rows) {
  const out = [];
  for (var at = 0; at < rows.length; at++) {
    out.push(function later() {
      return at;
    });
  }
  return out;
}
export function prunes(rows) {
  for (const row of rows) {
    if (row < 0) {
      rows.splice(0, 1);
    }
  }
  return rows.length;
}
export async function bothWaits() {
  const first = await loadOne();
  const second = await loadTwo();
  return first + second;
}
export function guards(value) {
  globalThis.console.assert(value > 0);
  return value;
}
async function loadOne() {
  return 1;
}
async function loadTwo() {
  return 2;
}
let openTabs = 0;
export function tabsOpen() {
  return openTabs;
}
export async function paced() {
  await sleep(500);
  return openTabs;
}
async function sleep(ms) {
  return new Promise((resolve) => globalThis.setTimeout(resolve, ms));
}
export async function fires(rows) {
  loadOne();
  return rows.length;
}
