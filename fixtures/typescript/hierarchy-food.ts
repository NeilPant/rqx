export abstract class Food {
  abstract label(): string;
}

export class DogFood extends Food {
  label(): string { return "dog"; }
}

export class CatFood extends Food {
  label(): string { return "cat"; }
}
