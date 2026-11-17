export class Cart {
  items = [];
  total() {
    return Checkout.rate * this.items.length;
  }
}
export class Checkout {
  static rate = 3;
  lines() {
    const cart = new Cart();
    return cart.items;
  }
}
export class CsvWriter {
  open(path) { void path; }
  write(row) { void row; }
  close() { }
}
export class JsonEmitter {
  begin(path) { void path; }
  emit(row) { void row; }
  finish() { }
}
export let openSessions = 0;
warmTheCache();
export class Ledger {
  writer;
  constructor() {
    this.writer = new CsvWriter();
  }
  stampedAt() {
    return Date.now();
  }
  peek(other) {
    return other._items.length;
  }
}
function warmTheCache() {
  openSessions = 0;
}
