// Seed functions for the complexity precision corpus. Each annotation is the
// ground truth the inference is scored against; `space` counts the call stack.
#include <algorithm>
#include <vector>

// @complexity time=O(n) space=O(1) conf=verified scope=local
long long sum_all(const std::vector<int>& values) {
    long long total = 0;
    for (int value : values) {
        total += value;
    }
    return total;
}

// @complexity time=O(n^2) space=O(1) conf=verified scope=local
bool has_duplicate(const std::vector<int>& values) {
    for (std::size_t i = 0; i < values.size(); ++i) {
        for (std::size_t j = i + 1; j < values.size(); ++j) {
            if (values[i] == values[j]) {
                return true;
            }
        }
    }
    return false;
}

// @complexity time=O(n*m) space=O(1) conf=verified scope=local
int count_matches(const std::vector<int>& left, const std::vector<int>& right) {
    int matches = 0;
    for (int a : left) {
        for (int b : right) {
            if (a == b) {
                ++matches;
            }
        }
    }
    return matches;
}

// @complexity time=O(n log n) space=O(n) conf=verified scope=local
int median(const std::vector<int>& values) {
    std::vector<int> ordered = values;
    std::sort(ordered.begin(), ordered.end());
    return ordered[ordered.size() / 2];
}
