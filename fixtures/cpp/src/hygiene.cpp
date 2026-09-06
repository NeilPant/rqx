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
}

int mutates_parameter(int value) {
    value = 1;
    return value;
}

std::string ship(const Order& order) {
    return order.customer().address().city().label();
}

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

int narrated(int rows) {
    // set the total
    int total = 0;
    // double it
    total = rows * 2;
    // halve it
    total = total / 2;
    // give it back
    return total;
}

// Works out the score for the given user id, and gives back the score it
// worked out, having looked at the id that was handed in by whoever called
// it, which is the id of the user whose score is wanted here. The id is not
// checked, because whoever calls this has already checked it, and checking
// twice would be slower for no benefit anybody has ever measured. If the id
// is wrong the answer is wrong, which is the same as it has always been in
// every version of this that anybody has written since the beginning.
int score_of(int user_id) {
    return user_id;
}

int score_of_user(int user_id) {
    return user_id;
}

int guarded_add(int a, int b, int c) {
    if (a < 0) return 0;
    if (b < 0) return 0;
    if (c < 0) return 0;
    return a + b + c;
}

int netted(int rows) {
    try {
        int total = 0;
        total = rows * 2;
        return total;
    } catch (const std::runtime_error& error) {
        return 0;
    }
}
