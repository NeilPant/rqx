// A function that calls another one, in the cases the cost rules tell apart.
#include <string>
#include <vector>

// @complexity time=O(n) space=O(1) conf=verified scope=local
int index_of_item(const std::vector<std::string>& list, const std::string& wanted) {
    for (size_t i = 0; i < list.size(); i++) {
        if (list[i] == wanted) {
            return static_cast<int>(i);
        }
    }
    return -1;
}

// @complexity time=O(n^2) space=O(n) conf=verified rule=quadratic-via-call
std::vector<std::string> first_occurrences(const std::vector<std::string>& items) {
    std::vector<std::string> firsts;
    for (size_t at = 0; at < items.size(); at++) {
        if (index_of_item(items, items[at]) == static_cast<int>(at)) {
            firsts.push_back(items[at]);
        }
    }
    return firsts;
}

// @complexity time=O(1) space=O(1) conf=verified scope=local
int scale(int x) {
    return x * 2;
}

// @complexity time=O(n) space=O(n) conf=verified none=quadratic-via-call
std::vector<int> scale_each(const std::vector<int>& items) {
    std::vector<int> out;
    for (int item : items) {
        out.push_back(scale(item));
    }
    return out;
}

// @complexity time=O(n) space=O(1) conf=verified scope=local
int total_length(const std::vector<std::string>& parts) {
    int total = 0;
    for (const std::string& part : parts) {
        total += static_cast<int>(part.size());
    }
    return total;
}

// @complexity time=O(n*m) space=O(m) conf=verified rule=repeated-computation-in-loop
std::vector<double> shares(const std::vector<std::string>& parts, const std::vector<double>& weights) {
    std::vector<double> out;
    for (double weight : weights) {
        int whole = total_length(parts);
        out.push_back(weight / whole);
    }
    return out;
}

// @complexity time=O(n) space=O(n) conf=verified none=repeated-computation-in-loop
std::vector<double> shares_of_each(const std::vector<double>& weights) {
    std::vector<double> out;
    for (double weight : weights) {
        out.push_back(weight / total_length({"x"}));
    }
    return out;
}

// @complexity time=O(n) space=O(n) conf=verified rule=deep-recursion
int sum_to(int n) {
    if (n <= 0) {
        return 0;
    }
    return n + sum_to(n - 1);
}

// @complexity time=O(log n) space=O(log n) conf=verified none=deep-recursion
int bits(int n) {
    if (n <= 1) {
        return 0;
    }
    return 1 + bits(n >> 1);
}

// @complexity time=O(n^2) space=O(1) conf=verified scope=local none=pair-scan-could-hash
int count_pairs(const std::vector<int>& items) {
    int pairs = 0;
    for (int left : items) {
        for (int right : items) {
            if (left == right) {
                pairs += 1;
            }
        }
    }
    return pairs;
}
