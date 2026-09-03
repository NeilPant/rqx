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
