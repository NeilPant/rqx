#include "precision.hpp"

std::string widen(void* input) {
    std::string raw = *((std::string*) input);
    return raw;
}


int stamp() { return 1; }
