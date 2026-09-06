package rqx;

abstract class Transport {
    abstract String deliver();
}

class Road extends Transport {
    String deliver() {
        return "road";
    }
}

class Rail extends Transport {
    String deliver() {
        return "rail";
    }
}

abstract class Logistics {
    abstract Transport createTransport();

    public String plan() {
        return this.createTransport().deliver();
    }
}

class RoadLogistics extends Logistics {
    Transport createTransport() {
        return new Road();
    }
}

class RailLogistics extends Logistics {
    Transport createTransport() {
        return new Rail();
    }
}

abstract class Button {
    abstract String render();
}

class FlatButton extends Button {
    String render() {
        return "flat";
    }
}

abstract class Panel {
    abstract String render();
}

class FlatPanel extends Panel {
    String render() {
        return "flat";
    }
}

abstract class Kit {
    abstract Button button();

    abstract Panel panel();
}

class DarkKit extends Kit {
    Button button() {
        return new FlatButton();
    }

    Panel panel() {
        return new FlatPanel();
    }
}

class LightKit extends Kit {
    Button button() {
        return new FlatButton();
    }

    Panel panel() {
        return new FlatPanel();
    }
}

abstract class Shape {
    abstract String draw();
}

class RedCircle extends Shape {
    String draw() {
        return "rc";
    }
}

class RedSquare extends Shape {
    String draw() {
        return "rs";
    }
}

class BlueCircle extends Shape {
    String draw() {
        return "bc";
    }
}

class BlueSquare extends Shape {
    String draw() {
        return "bs";
    }
}

abstract class Expression {
    abstract int eval();
}

class Literal extends Expression {
    int eval() {
        return 1;
    }
}

class Sum extends Expression {
    int eval() {
        return 2;
    }
}

class Product extends Expression {
    int eval() {
        return 3;
    }
}

class Printer {
    public String visitLiteral() {
        return "1";
    }

    public String visitSum() {
        return "+";
    }

    public String visitProduct() {
        return "*";
    }
}
