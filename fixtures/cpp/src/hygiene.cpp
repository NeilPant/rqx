#include "hygiene.hpp"
#include "../../../shared/far.hpp"
#include <cstdio>
using namespace std;
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

void nothing_happens() {
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


int orbit_of(int seed) {
    return seed + 1;
}

int forward_orbit(int seed) {
    return orbit_of(seed);
}

int still_to_write() {
    throw std::runtime_error("not implemented");
}

int reraises() {
    try {
        return orbit_of(1);
    } catch (const std::exception& error) {
        throw error;
    }
}

void traced() {
    printf("traced\n");
}

int maybe_count(int rows) {
    if (rows == 0) {
        return 0;
    }
    return rows;
}

int catches_everything(int rows) {
    try {
        return rows;
    } catch (...) {
        return 0;
    }
}

int refuses() {
    throw std::exception();
}

void stops(int code) {
    exit(code);
}

const char *requests(const char *url) {
    return fetch_page(url, true);
}

const char *fetch_page(const char *url, bool retry) {
    return retry ? url : url;
}
