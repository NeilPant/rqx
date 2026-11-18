// Library calls: an algorithm is sized by the container its first iterator belongs to,
// a member is costed from the table by the declared type, and a hashed lookup costs
// nothing. `scope=local` marks the functions whose own time the per-file extraction is
// held to.
#include <algorithm>
#include <list>
#include <map>
#include <set>
#include <string>
#include <unordered_set>
#include <vector>

// @complexity time=O(n log n) space=O(1) conf=verified scope=local
void sort_in_place(std::vector<int>& values) {
    std::sort(values.begin(), values.end());
}

// @complexity time=O(n*m) space=O(1) conf=verified rule=scan-in-loop scope=local
int contains_any(const std::vector<int>& values, const std::vector<int>& wanted) {
    int hits = 0;
    for (int value : values) {
        if (std::find(wanted.begin(), wanted.end(), value) != wanted.end()) {
            ++hits;
        }
    }
    return hits;
}

// @complexity time=O(n) space=O(1) conf=verified none=linear-op-in-loop scope=local
int contains_any_hashed(const std::vector<int>& values, const std::unordered_set<int>& wanted) {
    int hits = 0;
    for (int value : values) {
        if (wanted.find(value) != wanted.end()) {
            ++hits;
        }
    }
    return hits;
}

// @complexity time=O(n log m) space=O(1) conf=verified none=linear-op-in-loop scope=local
int contains_any_ordered(const std::vector<int>& values, const std::set<int>& wanted) {
    int hits = 0;
    for (int value : values) {
        if (wanted.count(value) > 0) {
            ++hits;
        }
    }
    return hits;
}

// @complexity time=O(n*m) space=O(1) conf=verified rule=linear-op-in-loop scope=local
void push_front_each(std::vector<int>& target, const std::vector<int>& values) {
    for (int value : values) {
        target.insert(target.begin(), value);
    }
}

// @complexity time=O(n) space=O(1) conf=verified none=linear-op-in-loop scope=local
void push_front_list(std::list<int>& target, const std::vector<int>& values) {
    for (int value : values) {
        target.insert(target.begin(), value);
    }
}

// @complexity time=O(n*m) space=O(1) conf=verified rule=scan-in-loop scope=local
int mentions(const std::string& text, const std::vector<std::string>& words) {
    int hits = 0;
    for (const auto& word : words) {
        if (text.find(word) != std::string::npos) {
            ++hits;
        }
    }
    return hits;
}

// @complexity time=O(n) space=O(n) conf=verified scope=local
std::vector<int> squares(const std::vector<int>& values) {
    std::vector<int> out;
    for (int v : values) {
        out.push_back(v * v);
    }
    return out;
}
