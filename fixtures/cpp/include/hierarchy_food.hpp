#pragma once
#include <string>

class Food {
public:
    virtual std::string label() = 0;
};

class DogFood : public Food {
public:
    std::string label() { return "dog"; }
};

class CatFood : public Food {
public:
    std::string label() { return "cat"; }
};
