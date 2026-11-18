// Code that resembles a finding and is not one. Every rule should stay silent here.
import { EventEmitter } from "node:events";

// distant-declaration, short-name-wide-scope: `tz` belongs to the module, so being
// read at the bottom of the file is where it belongs.
const tz = "UTC";

// output-parameter: `factor` in `counted` is a local that shares its name with
// this parameter, not this parameter.
export function scaled(factor) {
  return factor * 2;
}

export function counted(items) {
  let factor = items.length;
  factor += 1;
  return factor;
}

// ambient-dependency: the clock and the random source are parameters, and the
// ambient ones are only their defaults.
export function delay(base, random = Math.random, clock = () => Date.now()) {
  return base * random() + clock();
}

// mixed-language-file: an error message that names an update is not SQL.
export function updateFailure(id) {
  return "Failed to update user " + id;
}

export function zoneLabel() {
  return "zone " + tz;
}

// magic-number: each 30 below is the value of a declaration, which names it.
const CONNECT_TIMEOUT_SECONDS = 30;
const READ_TIMEOUT_SECONDS = 30;
const SOCKET_TIMEOUT_SECONDS = -30;

export function timeoutBudget() {
  return CONNECT_TIMEOUT_SECONDS + READ_TIMEOUT_SECONDS - SOCKET_TIMEOUT_SECONDS;
}

// import-time-work: registering a handler runs nothing until the event arrives.
export const events = new EventEmitter();

function shout(payload) {
  return payload.toUpperCase();
}

events.on("shout", shout);
events.on("trim", (payload) => payload.trim());

// no-base-case: the method calls the function it is named after, not itself.
function render(text) {
  return text.trim();
}

export class Page {
  render(text) {
    return render(text) + "\n";
  }
}

// closure-over-loop-variable: `replace` calls the function before it returns.
export function suffixed(text, suffixes) {
  const out = [];
  for (const suffix of suffixes) {
    out.push(text.replace(/o/g, (match) => match + suffix));
  }
  return out;
}
