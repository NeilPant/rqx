#pragma once
#include <string>

class Transport {
public:
    virtual std::string deliver() = 0;
};

class Road : public Transport {
public:
    std::string deliver() { return "road"; }
};

class Rail : public Transport {
public:
    std::string deliver() { return "rail"; }
};

class Logistics {
public:
    virtual Transport* create_transport() = 0;
    std::string plan() { return this->create_transport()->deliver(); }
};

class RoadLogistics : public Logistics {
public:
    Transport* create_transport() { return new Road(); }
};

class RailLogistics : public Logistics {
public:
    Transport* create_transport() { return new Rail(); }
};

class Button {
public:
    virtual std::string render() = 0;
};

class FlatButton : public Button {
public:
    std::string render() { return "flat"; }
};

class Panel {
public:
    virtual std::string render() = 0;
};

class FlatPanel : public Panel {
public:
    std::string render() { return "flat"; }
};

class Kit {
public:
    virtual Button* button() = 0;
    virtual Panel* panel() = 0;
};

class DarkKit : public Kit {
public:
    Button* button() { return new FlatButton(); }
    Panel* panel() { return new FlatPanel(); }
};

class LightKit : public Kit {
public:
    Button* button() { return new FlatButton(); }
    Panel* panel() { return new FlatPanel(); }
};

class Shape {
public:
    virtual std::string draw() = 0;
};

class RedCircle : public Shape {
public:
    std::string draw() { return "rc"; }
};

class RedSquare : public Shape {
public:
    std::string draw() { return "rs"; }
};

class BlueCircle : public Shape {
public:
    std::string draw() { return "bc"; }
};

class BlueSquare : public Shape {
public:
    std::string draw() { return "bs"; }
};

class Expression {
public:
    virtual int eval() = 0;
};

class Literal : public Expression {
public:
    int eval() { return 1; }
};

class Sum : public Expression {
public:
    int eval() { return 2; }
};

class Product : public Expression {
public:
    int eval() { return 3; }
};

class Printer {
public:
    std::string visit_literal(Literal* node) { return "1"; }
    std::string visit_sum(Sum* node) { return "+"; }
    std::string visit_product(Product* node) { return "*"; }
};
