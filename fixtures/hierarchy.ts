export abstract class Feeder {
  abstract feed(): string;
}

export class DogFeeder extends Feeder {
  feed(): string { return "kibble"; }
}

export class CatFeeder extends Feeder {
  feed(): string { return "fish"; }
}
