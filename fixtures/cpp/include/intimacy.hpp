#pragma once
#include <string>
#include <vector>

class Checkout;

class Cart {
public:
    std::vector<std::string> items;
    int total() {
        return Checkout::rate * static_cast<int>(this->items.size());
    }
};

class Checkout {
public:
    static int rate;
    std::vector<std::string> lines() {
        Cart cart;
        return cart.items;
    }
};

class CsvWriter {
public:
    void open(const std::string& path) {}
    void write(const std::string& row) {}
    void close() {}
};

class JsonEmitter {
public:
    void begin(const std::string& path) {}
    void emit(const std::string& row) {}
    void finish() {}
};
