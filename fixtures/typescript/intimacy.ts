export class Cart {
  items: string[] = [];

  total(): number {
    return Checkout.rate * this.items.length;
  }
}

export class Checkout {
  static rate = 3;

  lines(): string[] {
    const cart = new Cart();
    return cart.items;
  }
}

export class CsvWriter {
  open(path: string): void { void path; }
  write(row: string): void { void row; }
  close(): void {}
}

export class JsonEmitter {
  begin(path: string): void { void path; }
  emit(row: string): void { void row; }
  finish(): void {}
}

export let openSessions = 0;

warmTheCache();

export class Ledger {
  private writer: CsvWriter;
  constructor() {
    this.writer = new CsvWriter();
  }
  stampedAt(): number {
    return Date.now();
  }
  peek(other: Cart & { _items: string[] }): number {
    return other._items.length;
  }
}

function warmTheCache(): void {
  openSessions = 0;
}
