#include "params.hpp"

int with_dead_tail(int used, const std::string& dead) {
    return used;
}

int with_dead_middle(const std::string& dead, int used) {
    return used;
}

int respects_underscore(int used, const std::string& _deliberate) {
    return used;
}

int two_dead_on_one_line(int used, const std::string& deadA, const std::string& deadB) {
    return used;
}
