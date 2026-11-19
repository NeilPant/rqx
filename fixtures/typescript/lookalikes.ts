// Code that resembles a finding and is not one. Every rule should stay silent here.
import { EventEmitter } from "node:events";

// distant-declaration, short-name-wide-scope: `tz` belongs to the module, so being
// read at the bottom of the file is where it belongs.
const tz = "UTC";

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

export function zoneLabel(): string {
  return "zone " + tz;
}

// magic-number: each 30 below is the value of a declaration, which names it.
// hungarian-notation: IDLE_… is a constant in capitals, not an interface prefix.
const CONNECT_TIMEOUT_SECONDS = 30;
const READ_TIMEOUT_SECONDS = 30;
const IDLE_TIMEOUT_SECONDS = -30;

export function timeoutBudget(): number {
  return CONNECT_TIMEOUT_SECONDS + READ_TIMEOUT_SECONDS - IDLE_TIMEOUT_SECONDS;
}

// import-time-work: registering a handler runs nothing until the event arrives.
export const events = new EventEmitter();

function shout(payload: string): string {
  return payload.toUpperCase();
}

events.on("shout", shout);
events.on("trim", (payload: string) => payload.trim());
events.on("loud", suffixer("!"));

function suffixer(ending: string): (payload: string) => string {
  return (payload) => payload + ending;
}

// unused-parameter: `value` belongs to the function type the result is declared as.
export function adder(step: number): (value: number) => number {
  return (current) => current + step;
}

// no-base-case: the method calls the function it is named after, not itself.
function render(text: string): string {
  return text.trim();
}

export class Page {
  render(text: string): string {
    return render(text) + "\n";
  }
}

// closure-over-loop-variable: `replace` calls the function before it returns.
export function suffixed(text: string, suffixes: string[]): string[] {
  const out: string[] = [];
  for (const suffix of suffixes) {
    out.push(text.replace(/o/g, (match) => match + suffix));
  }
  return out;
}

// closure-over-loop-variable: `let` and `const` give each pass a binding of its own.
export function laterEach(rows: number[]): Array<() => number> {
  const out: Array<() => number> = [];
  for (const row of rows) {
    out.push(() => row);
  }
  for (let at = 0; at < rows.length; at++) {
    out.push(() => at);
  }
  return out;
}

// imprecise-type: a method or a variable called `any` is not the `any` type.
type Counter = { any(test: (value: number) => boolean): number };

export function hasOdd(values: Counter): number {
  let any = values.any((value) => value % 2 === 1);
  any += values.any((value) => value > 0);
  return any;
}

// distant-declaration: the table is read while it is declared, and used on the next line.
export function spelled(word: string): number {
  const codeWords = [
    "alfa",
    "bravo",
    "charlie",
    "delta",
    "echo",
    "foxtrot",
    "golf",
    "hotel",
    "india",
    "juliett",
    "kilo",
    "lima",
    "mike",
    "november",
    "oscar",
    "papa",
    "quebec",
    "romeo",
    "sierra",
    "tango",
    "uniform",
    "victor",
    "whiskey",
    "xray",
    "yankee",
    "zulu",
  ];
  return codeWords.indexOf(word);
}

// empty-method: the constructor's work is its parameter property.
export class Stamp {
  constructor(private readonly label: string) {}

  describe(): string {
    return "[" + this.label + "]";
  }
}

function subscribe(handler: () => void): () => void {
  events.on("close", handler);
  return () => events.off("close", handler);
}

// no-base-case: the callback calls the function `subscribe` returned, not itself.
export function listenOnce(): void {
  const unsubscribe = subscribe(() => {
    unsubscribe();
  });
}

// focused-test: `fit` is this file's own function, and fitting a width is not a test.
function fit(width: number, limit: number): number {
  return Math.max(0, Math.min(width, limit));
}

export function fitted(widths: number[], limit: number): number[] {
  return widths.map((width) => fit(width, limit));
}

// empty-catch: a catch that says why it does nothing has handled the error.
export function parsedOrNothing(text: string): unknown {
  try {
    return JSON.parse(text);
  } catch {
    // Malformed input means no settings, which the caller already handles.
  }
  return undefined;
}

// unreachable-code: a `var` is hoisted to the top of its function, so the one after
// the return is the `total` the first line assigns.
export function hoistedTotal(): number {
  total = 1;
  return total;
  var total: number;
}

// self-comparison: only NaN differs from itself, so this is how NaN is detected.
export function isMissing(reading: number): boolean {
  return reading !== reading;
}

export function drained(pending: number[]): number {
  // exit-from-finally: the break and continue stay in a loop inside the clause.
  try {
    return pending.length;
  } finally {
    while (pending.length > 0) {
      const last = pending.pop();
      if (last === undefined) {
        continue;
      }
      if (last < 0) {
        break;
      }
    }
  }
}
