#pragma once
#include <string>
#include <vector>

int pricing(const std::vector<int>& cart, bool apply_discount);
int unreachable(int value);
int mutates_parameter(int value);

class City {
public:
    std::string label() const;
};

class Address {
public:
    City city() const;
};

class Customer {
public:
    Address address() const;
};

class Order {
public:
    Customer customer() const;
};

std::string ship(const Order& order);
