#pragma once
#include <string>

class Feeder {
public:
    virtual std::string feed() = 0;
};

class DogFeeder : public Feeder {
public:
    std::string feed() { return "kibble"; }
};

class CatFeeder : public Feeder {
public:
    std::string feed() { return "fish"; }
};
