#include "hygiene.hpp"

int pricing(const std::vector<int>& cart, bool apply_discount) {
    int never_read = static_cast<int>(cart.size());
    int total = 0;
    for (int item : cart) {
        total += item;
    }

    // int legacy = total * 2;
    // return legacy + shipping;

    if (!apply_discount && total > 4999 && cart.size() > 12 && total != 7) {
        return total;
    } else {
        total = total * 4999;
    }
    try {
        return total + 4999;
    } catch (const std::exception& error) {
    }
    return 0;
}

int unreachable(int value) {
    // rqx-ignore magic-number
    int seed = 1;
    return value + seed;
    value = 1;
}

int mutates_parameter(int value) {  // NOLINT
    value = 1;
    return value;
}

std::string ship(const Order& order) {
    return order.customer().address().city().label();
}

// ---------------- helpers ----------------
// Here's the helper you asked for! 🎉
// Sorry, this one is ugly.

