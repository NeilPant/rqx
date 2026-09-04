#include "config.hpp"
#include <algorithm>

std::string endpoint(const std::string& region, bool secure, int retries) {
    if (region.size() > 0) {
        if (secure) {
            if (retries > 0) {
                return "https://api.example.com/v2/orders";
            }
        }
    }
    return "";
}

int score(int a, int b, int c, int d) {
    return (a + b) * (c - d) + std::max(a, b) + std::min(c, d) - (a * d);
}
