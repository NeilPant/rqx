// A function in a header that a loop in another file calls for every element.
#pragma once
#include <string>
#include <vector>

// @complexity time=O(n) space=O(1) conf=verified
inline int find_at(const std::vector<std::string>& items, const std::string& wanted) {
    for (size_t at = 0; at < items.size(); at++) {
        if (items[at] == wanted) {
            return static_cast<int>(at);
        }
    }
    return -1;
}
