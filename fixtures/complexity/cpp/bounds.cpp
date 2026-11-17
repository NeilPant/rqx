// Loop bounds, one class each: a fixed number, the size of a collection or a count, a
// logarithm, and the shapes that must not change the answer. `scope=local` marks the
// functions whose own time the per-file extraction is held to.
#include <algorithm>
#include <unordered_map>
#include <vector>

constexpr int WINDOW = 8;

// @complexity time=O(1) space=O(1) conf=verified scope=local
int window_sum(const std::vector<int>& values) {
    int total = 0;
    for (int i = 0; i < WINDOW; ++i) {
        total += values[i];
    }
    return total;
}

// @complexity time=O(n) space=O(1) conf=verified scope=local
int count_up(int n) {
    int total = 0;
    for (int i = 0; i < n; ++i) {
        total += i;
    }
    return total;
}

// @complexity time=O(log n) space=O(1) conf=verified scope=local
int halvings(int n) {
    int steps = 0;
    while (n > 1) {
        n /= 2;
        ++steps;
    }
    return steps;
}

// @complexity time=O(log n) space=O(1) conf=verified scope=local
int powers_below(int limit) {
    int count = 0;
    for (int p = 1; p < limit; p *= 2) {
        ++count;
    }
    return count;
}

// @complexity time=O(n^2) space=O(1) conf=verified scope=local
int count_inversions(const std::vector<int>& values) {
    int inversions = 0;
    for (std::size_t i = 0; i < values.size(); ++i) {
        for (std::size_t j = i + 1; j < values.size(); ++j) {
            if (values[i] > values[j]) {
                ++inversions;
            }
        }
    }
    return inversions;
}

// @complexity time=O(n) space=O(1) conf=verified scope=local
int first_negative(const std::vector<int>& values) {
    for (int value : values) {
        if (value < 0) {
            return value;
        }
    }
    return 0;
}

// @complexity time=O(log n) space=O(1) conf=verified scope=local
int lower_bound_index(const std::vector<int>& sorted, int target) {
    std::size_t low = 0;
    std::size_t high = sorted.size();
    while (low < high) {
        std::size_t mid = (low + high) / 2;
        if (sorted[mid] < target) {
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    return static_cast<int>(low);
}

// @complexity time=O(n) space=O(1) conf=verified scope=local
int sum_values(const std::unordered_map<int, int>& table) {
    int total = 0;
    for (const auto& entry : table) {
        total += entry.second;
    }
    return total;
}

// @complexity time=O(n) space=O(1) conf=verified scope=local
int sum_each(const std::vector<int>& values) {
    int total = 0;
    std::for_each(values.begin(), values.end(), [&total](int v) { total += v; });
    return total;
}
