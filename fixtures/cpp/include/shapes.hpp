#pragma once
#include <string>
#include <vector>

class Coordinate {
public:
    int x = 0;
    int y = 0;
    std::vector<int> pair() { return {x, y}; }
};

class Marker {
public:
    std::string label = "x";
};

class Shipment {
public:
    std::string strLabel = "";
};

class Facade {
private:
    Coordinate inner;
public:
    std::vector<int> a() { return this->inner.pair(); }
    int b() { return this->inner.x; }
};

class Vehicle {
public:
    virtual std::string go() = 0;
};

class Engine {
private:
    int unusedSecret = 1;
public:
    std::string scratch;
    std::string run() {
        this->scratch = "on";
        return this->scratch;
    }
    static Engine build(Engine other) { return other; }
};

class Broken : public Engine {
public:
    std::string run() {
        throw std::string("no");
    }
};

struct Basket {
    int a = 0;
    int b = 0;
    int c = 0;
};

class Totals {
public:
    int add(Basket basket) {
        return basket.a + basket.b + basket.c + basket.a;
    }
};

class CachedRegistry;

class Registry {
public:
    int build(const std::string& kind) {
        return kind == "cached" ? sizeof(CachedRegistry) : 0;
    }
};

class CachedRegistry : public Registry {
};

int route_of(const std::string& kind);
int price_of(const std::string& kind);
std::string notify_of(const std::string& user_id, const std::string& channel, const std::string& locale);
std::string enqueue(const std::string& user_id, const std::string& channel, const std::string& locale);
std::string record_of(const std::string& user_id, const std::string& channel, const std::string& locale);
int gate(bool ready, int retries);
int sweep(const std::vector<int>& rows);
