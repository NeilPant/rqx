export class Transport {
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
export class Logistics {
  plan() {
    return this.createTransport().deliver();
  }
}
export class RoadLogistics extends Logistics {
  createTransport() {
    return new Road();
  }
}
export class RailLogistics extends Logistics {
  createTransport() {
    return new Rail();
  }
}
export class Button {
}
export class FlatButton extends Button {
  render() {
    return "flat";
  }
}
export class Panel {
}
export class FlatPanel extends Panel {
  render() {
    return "flat";
  }
}
export class Kit {
}
export class DarkKit extends Kit {
  button() {
    return new FlatButton();
  }
  panel() {
    return new FlatPanel();
  }
  toolbar() {
    return new Button();
  }
}
export class LightKit extends Kit {
  button() {
    return new FlatButton();
  }
  panel() {
    return new FlatPanel();
  }
}
export class Shape {
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
export class Expression {
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
  visitLiteral(node) {
    return "1";
  }
  visitSum(node) {
    return "+";
  }
  visitProduct(node) {
    return "*";
  }
}
