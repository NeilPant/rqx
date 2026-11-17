export class Door {
  state = "closed";
  open() {
    switch (this.state) {
      case "closed":
        this.state = "open";
        return true;
      case "open":
        return false;
      case "locked":
        return false;
      default:
        return false;
    }
  }
  lock(key) {
    if (this.state === "open") {
      return false;
    }
    else if (this.state === "locked") {
      return false;
    }
    else if (this.state === "closed") {
      this.state = "locked";
      return key.length > 0;
    }
    return false;
  }
}
function isHealthCheck(request) {
  return request.path === "/health";
}
function looksLikeUpload(request) {
  return request.body.length > 1024 && request.path.startsWith("/files");
}
function hasSession(request) {
  return request.session !== "";
}
export function route(request) {
  if (isHealthCheck(request)) {
    return "ok";
  }
  else if (looksLikeUpload(request)) {
    return "stored";
  }
  else if (hasSession(request)) {
    return "page";
  }
  return "denied";
}
function addItem(action) {
  return action.payload.length;
}
function removeItem(action) {
  return -action.payload.length;
}
function clearCart(action) {
  return action.payload.indexOf("x");
}
export function apply(action) {
  switch (action.kind) {
    case "add":
      return addItem(action);
    case "remove":
      return removeItem(action);
    case "clear":
      return clearCart(action);
  }
  return 0;
}
export function replay(action) {
  switch (action.kind) {
    case "add":
      return addItem(action);
  }
  return 0;
}
export class LoggingNotifier {
  inner;
  constructor(inner) {
    this.inner = inner;
  }
  send(message) {
    return this.inner.send(message);
  }
  close() {
    return this.inner.close();
  }
  history() {
    return 1;
  }
}
export class GuardedNotifier {
  inner;
  constructor(inner) {
    this.inner = inner;
  }
  send(message) {
    return this.inner.send(message);
  }
  close() {
    return this.inner.close();
  }
}
export class SmsGateway {
  transmit(text) {
    return text.length;
  }
  disconnect() {
    return 0;
  }
}
export class SmsNotifier {
  gateway;
  constructor(gateway) {
    this.gateway = gateway;
  }
  send(message) {
    return this.gateway.transmit(message);
  }
  close() {
    return this.gateway.disconnect();
  }
}
export class Menu {
  items = [];
  attach(child) {
    this.items.push(child);
  }
  size() {
    return this.items.length;
  }
}
export class ReportBuilder {
  title = "";
  withTitle(title) {
    this.title = title;
    return this;
  }
  withRows(rows) {
    return this;
  }
  withFooter(footer) {
    return this;
  }
  build() {
    return this.title;
  }
}
export class QueryBuilder {
  clauses = 0;
  where(field) {
    this.clauses += 1;
    return this;
  }
  order(field) {
    return this;
  }
  take(count) {
    return this;
  }
}
export class Palette {
  colours = 3;
  clone() {
    const copy = new Palette();
    copy.colours = this.colours;
    return copy;
  }
}
export class Settings {
  static #instance;
  constructor() { }
  static shared() {
    return Settings.#instance;
  }
}
export class Clock {
  static #current;
  constructor() { }
  static now() {
    return Clock.#current;
  }
}
export class Glyph {
  static #pool = [];
  static of(index) {
    return Glyph.#pool[index];
  }
}
export class Pages {
  pages = [];
  next() {
    return this.pages[0];
  }
}
export class Feed {
  listeners = [];
  addListener(handler) {
    this.listeners.push(handler);
  }
  removeListener(handler) {
    this.listeners.pop();
  }
  publish(value) {
    this.listeners.forEach((listener) => listener(value));
  }
}
export class Ticker {
  handlers = [];
  onTick(handler) {
    this.handlers.push(handler);
  }
  fire() {
    this.handlers.forEach((handler) => handler());
  }
}
export class Editor {
  text = "";
  save() {
    return this.text;
  }
  restore(state) {
    this.text = state;
  }
}
export class Importer {
  run() {
    const row = this.read();
    this.write(row);
    return row;
  }
}
export class RetryingGateway {
  gateway;
  constructor(gateway) {
    this.gateway = gateway;
  }
  transmit(text) {
    return this.gateway.transmit(text);
  }
  disconnect() {
    return this.gateway.disconnect();
  }
}
