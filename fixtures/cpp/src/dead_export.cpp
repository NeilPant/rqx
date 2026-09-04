#include "dead_export.hpp"

int reachable() {
    return 1;
}

int never_included(int value) {
    return value * 2;
}
