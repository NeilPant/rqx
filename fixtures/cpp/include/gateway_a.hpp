#pragma once
#include <stripe/charge.hpp>

inline int collect_a(int amount) {
    return stripe::charge(amount);
}
