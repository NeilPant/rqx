export abstract class Transport {
  abstract deliver(): string;
}

export class Road extends Transport {
  deliver() {
    return "road";
  }
}

export class Rail extends Transport {
  deliver() {
    return "rail";
  }
}

export abstract class Logistics {
  abstract createTransport(): Transport;
  plan() {
    return this.createTransport().deliver();
  }
}

export class RoadLogistics extends Logistics {
  createTransport(): Transport {
    return new Road();
  }
}

export class RailLogistics extends Logistics {
  createTransport(): Transport {
    return new Rail();
  }
}

export abstract class Button {
  abstract render(): string;
}

export class FlatButton extends Button {
  render() {
    return "flat";
  }
}

export abstract class Panel {
  abstract render(): string;
}

export class FlatPanel extends Panel {
  render() {
    return "flat";
  }
}

export abstract class Kit {
  abstract button(): Button;
  abstract panel(): Panel;
}

export class DarkKit extends Kit {
  button(): Button {
    return new FlatButton();
  }
  panel(): Panel {
    return new FlatPanel();
  }
}

export class LightKit extends Kit {
  button(): Button {
    return new FlatButton();
  }
  panel(): Panel {
    return new FlatPanel();
  }
}

export abstract class Shape {
  abstract draw(): string;
}

export class RedCircle extends Shape {
  draw() {
    return "rc";
  }
}

export class RedSquare extends Shape {
  draw() {
    return "rs";
  }
}

export class BlueCircle extends Shape {
  draw() {
    return "bc";
  }
}

export class BlueSquare extends Shape {
  draw() {
    return "bs";
  }
}

export abstract class Expression {
  abstract eval(): number;
}

export class Literal extends Expression {
  eval() {
    return 1;
  }
}

export class Sum extends Expression {
  eval() {
    return 2;
  }
}

export class Product extends Expression {
  eval() {
    return 3;
  }
}

export class Printer {
  visitLiteral(node: Literal) {
    return "1";
  }
  visitSum(node: Sum) {
    return "+";
  }
  visitProduct(node: Product) {
    return "*";
  }
}
