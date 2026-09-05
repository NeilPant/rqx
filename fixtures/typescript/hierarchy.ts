export abstract class Feeder {
  abstract feed(): string;
}

export class DogFeeder extends Feeder {
  feed(): string { return "kibble"; }
}

export class CatFeeder extends Feeder {
  feed(): string { return "fish"; }
}

export class QuietFeeder extends Feeder {
  feed(): string {
    return super.feed();
  }
}

export function firstWay(rows: number[]): number {
  const total = rows.length;
  const doubled = total * 2;
  return doubled + 1;
}

export function secondWay(rows: number[]): number {
  const total = rows.length;
  const doubled = total * 2;
  return doubled + 1;
}

export function chained(rows: number[]): Promise<number> {
  return Promise.resolve(rows.length).then((count) => {
    return Promise.resolve(count).then((inner) => inner + 1);
  });
}
