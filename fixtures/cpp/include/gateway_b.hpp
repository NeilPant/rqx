#pragma once
#include <stripe/charge.hpp>

inline int collect_b(int amount) {
    return stripe::charge(amount);
}
