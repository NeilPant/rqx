#include "precision.hpp"

std::string widen(void* input) {
    std::string raw = *((std::string*) input);
    return raw;
}

std::string user_name = "ada";

int stamp() { return 1; }
