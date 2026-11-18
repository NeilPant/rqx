#include "search.hpp"

// The list searched is the list looped over, so the search is made once per element.

// @complexity time=O(n^2) space=O(n) conf=verified rule=quadratic-via-call
std::vector<std::string> first_occurrences(const std::vector<std::string>& items) {
    std::vector<std::string> firsts;
    for (size_t at = 0; at < items.size(); at++) {
        if (find_at(items, items[at]) == static_cast<int>(at)) {
            firsts.push_back(items[at]);
        }
    }
    return firsts;
}

// Each group is passed on as it is, so what it costs is what the group's own size costs.

// @complexity time=O(n*m) space=O(n) conf=verified none=quadratic-via-call
std::vector<int> positions_each(const std::vector<std::vector<std::string>>& groups, const std::string& wanted) {
    std::vector<int> found;
    for (const auto& group : groups) {
        found.push_back(find_at(group, wanted));
    }
    return found;
}
