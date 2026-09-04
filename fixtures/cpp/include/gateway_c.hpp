#pragma once
#include <stripe/charge.hpp>

inline int collect_c(int amount) {
    return stripe::charge(amount);
}
