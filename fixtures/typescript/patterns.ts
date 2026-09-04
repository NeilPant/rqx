export class Door {
  private state = "closed";

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

  lock(key: string) {
    if (this.state === "open") {
      return false;
    } else if (this.state === "locked") {
      return false;
    } else if (this.state === "closed") {
      this.state = "locked";
      return key.length > 0;
    }
    return false;
  }
}

interface Request {
  path: string;
  body: string;
  session: string;
}

function isHealthCheck(request: Request) {
  return request.path === "/health";
}

function looksLikeUpload(request: Request) {
  return request.body.length > 1024 && request.path.startsWith("/files");
}

function hasSession(request: Request) {
  return request.session !== "";
}

export function route(request: Request) {
  if (isHealthCheck(request)) {
    return "ok";
  } else if (looksLikeUpload(request)) {
    return "stored";
  } else if (hasSession(request)) {
    return "page";
  }
  return "denied";
}

interface Action {
  kind: string;
  payload: string;
}

function addItem(action: Action) {
  return action.payload.length;
}

function removeItem(action: Action) {
  return -action.payload.length;
}

function clearCart(action: Action) {
  return action.payload.indexOf("x");
}

export function apply(action: Action) {
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

export function replay(action: Action) {
  switch (action.kind) {
    case "add":
      return addItem(action);
  }
  return 0;
}

export interface Notifier {
  send(message: string): void;
  close(): void;
}

export class LoggingNotifier implements Notifier {
  constructor(private readonly inner: Notifier) {}
  send(message: string) {
    return this.inner.send(message);
  }
  close() {
    return this.inner.close();
  }
  history() {
    return 1;
  }
}

export class GuardedNotifier implements Notifier {
  constructor(private readonly inner: Notifier) {}
  send(message: string) {
    return this.inner.send(message);
  }
  close() {
    return this.inner.close();
  }
}

export class SmsGateway {
  transmit(text: string) {
    return text.length;
  }
  disconnect() {
    return 0;
  }
}

export class SmsNotifier implements Notifier {
  constructor(private readonly gateway: SmsGateway) {}
  send(message: string) {
    return this.gateway.transmit(message);
  }
  close() {
    return this.gateway.disconnect();
  }
}

export class Menu {
  private readonly items: Menu[] = [];
  attach(child: Menu) {
    this.items.push(child);
  }
  size() {
    return this.items.length;
  }
}

export class ReportBuilder {
  private title = "";
  withTitle(title: string): ReportBuilder {
    this.title = title;
    return this;
  }
  withRows(rows: number): ReportBuilder {
    return this;
  }
  withFooter(footer: string): ReportBuilder {
    return this;
  }
  build(): string {
    return this.title;
  }
}

export class QueryBuilder {
  private clauses = 0;
  where(field: string): QueryBuilder {
    this.clauses += 1;
    return this;
  }
  order(field: string): QueryBuilder {
    return this;
  }
  take(count: number): QueryBuilder {
    return this;
  }
}

export class Palette {
  colours = 3;
  clone(): Palette {
    const copy = new Palette();
    copy.colours = this.colours;
    return copy;
  }
}

export class Settings {
  private static instance: Settings;
  private constructor() {}
  static shared(): Settings {
    return Settings.instance;
  }
}

export class Clock {
  private static current: Clock;
  constructor() {}
  static now(): Clock {
    return Clock.current;
  }
}

export class Glyph {
  private static pool: Glyph[] = [];
  static of(index: number): Glyph {
    return Glyph.pool[index];
  }
}

export class Pages {
  private readonly pages: string[] = [];
  next(): string {
    return this.pages[0];
  }
}

export class Feed {
  private readonly listeners: Array<(value: number) => void> = [];
  addListener(handler: (value: number) => void) {
    this.listeners.push(handler);
  }
  removeListener(handler: (value: number) => void) {
    this.listeners.pop();
  }
  publish(value: number) {
    this.listeners.forEach((listener) => listener(value));
  }
}

export class Ticker {
  private readonly handlers: Array<() => void> = [];
  onTick(handler: () => void) {
    this.handlers.push(handler);
  }
  fire() {
    this.handlers.forEach((handler) => handler());
  }
}

export class Editor {
  private text = "";
  save(): string {
    return this.text;
  }
  restore(state: string) {
    this.text = state;
  }
}

export abstract class Importer {
  abstract read(): string;
  abstract write(row: string): void;
  run() {
    const row = this.read();
    this.write(row);
    return row;
  }
}

export class RetryingGateway {
  constructor(private readonly gateway: SmsGateway) {}
  transmit(text: string) {
    return this.gateway.transmit(text);
  }
  disconnect() {
    return this.gateway.disconnect();
  }
}
