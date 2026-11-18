// A tour of the library cost tables. Every function loops and calls members of one
// container, or algorithms over it, so what the table says a call costs is what the loop
// costs per pass. `scope=local` marks the functions whose own time the per-file
// extraction is held to.
#include <algorithm>
#include <deque>
#include <list>
#include <map>
#include <numeric>
#include <queue>
#include <set>
#include <string>
#include <unordered_map>
#include <unordered_set>
#include <vector>

// @complexity time=O(n*m) space=O(1) conf=verified rule=linear-op-in-loop may=scan-in-loop scope=local
void vector_edits(const std::vector<int>& values, std::vector<int>& items) {
    for (int value : values) {
        items.insert(items.begin(), value);
        items.erase(items.begin());
    }
}

// @complexity time=O(n*m) space=O(1) conf=verified rule=linear-op-in-loop may=scan-in-loop scope=local
void vector_resets(int rounds, std::vector<int>& items) {
    for (int i = 0; i < rounds; i++) {
        items.assign(items.size(), 0);
        items.resize(items.size());
        items.clear();
    }
}

// @complexity time=O(n) space=O(1) conf=verified none=linear-op-in-loop,scan-in-loop scope=local
int vector_constant_ops(const std::vector<int>& values, std::vector<int>& items) {
    int total = 0;
    for (int value : values) {
        items.push_back(value);
        items.emplace_back(value);
        total += items.at(0) + static_cast<int>(items.size());
        items.pop_back();
        items.pop_back();
    }
    return total;
}

// @complexity time=O(n) space=O(1) conf=verified none=linear-op-in-loop,scan-in-loop scope=local
int deque_ends(const std::vector<int>& values, std::deque<int>& queue) {
    int total = 0;
    for (int value : values) {
        queue.push_front(value);
        queue.push_back(value);
        queue.pop_front();
        queue.pop_back();
        total += queue.at(0);
    }
    return total;
}

// @complexity time=O(n*m) space=O(1) conf=verified rule=linear-op-in-loop may=scan-in-loop scope=local
void deque_edits(const std::vector<int>& values, std::deque<int>& queue) {
    for (int value : values) {
        queue.insert(queue.begin() + 1, value);
        queue.erase(queue.begin() + 1);
    }
}

// @complexity time=O(n) space=O(1) conf=verified none=linear-op-in-loop,scan-in-loop scope=local
void list_ends(const std::vector<int>& values, std::list<int>& chain) {
    for (int value : values) {
        chain.push_back(value);
        chain.push_front(value);
        chain.insert(chain.begin(), value);
        chain.erase(chain.begin());
        chain.pop_front();
        chain.pop_front();
    }
}

// @complexity time=O(n*m) space=O(1) conf=verified rule=linear-op-in-loop may=scan-in-loop scope=local
void list_passes(int rounds, std::list<int>& chain) {
    for (int i = 0; i < rounds; i++) {
        chain.unique();
        chain.reverse();
    }
}

// @complexity time=O(n*m) space=O(1) conf=verified rule=linear-op-in-loop may=scan-in-loop scope=local
void list_removals(const std::vector<int>& values, std::list<int>& chain) {
    for (int value : values) {
        chain.remove(value);
    }
}

// @complexity time=O(n*m log m) space=O(1) conf=verified rule=scan-in-loop may=linear-op-in-loop scope=local
void list_sorts(int rounds, std::list<int>& chain) {
    for (int i = 0; i < rounds; i++) {
        chain.sort();
    }
}

// @complexity time=O(n log m) space=O(1) conf=verified none=linear-op-in-loop,scan-in-loop scope=local
int ordered_map_ops(const std::vector<std::string>& keys, std::map<std::string, int>& table) {
    int total = 0;
    for (const std::string& key : keys) {
        table.insert({key, 1});
        table.emplace(key, 2);
        if (table.find(key) != table.end() && table.count(key) > 0 && table.contains(key)) {
            total += table.at(key);
        }
        total += static_cast<int>(table.lower_bound(key) == table.upper_bound(key));
        table.erase(key);
    }
    return total;
}

// @complexity time=O(n log m) space=O(1) conf=verified none=linear-op-in-loop,scan-in-loop scope=local
int ordered_set_ops(const std::vector<int>& values, std::set<int>& seen) {
    int total = 0;
    for (int value : values) {
        seen.insert(value);
        seen.emplace(value);
        if (seen.find(value) != seen.end() && seen.count(value) > 0 && seen.contains(value)) {
            total++;
        }
        total += static_cast<int>(seen.lower_bound(value) == seen.upper_bound(value));
        seen.erase(value);
    }
    return total;
}

// @complexity time=O(n) space=O(1) conf=verified none=linear-op-in-loop,scan-in-loop scope=local
int hashed_map_ops(const std::vector<std::string>& keys, std::unordered_map<std::string, int>& table) {
    int total = 0;
    for (const std::string& key : keys) {
        table.insert({key, 1});
        table.emplace(key, 2);
        if (table.find(key) != table.end() && table.count(key) > 0 && table.contains(key)) {
            total += table.at(key);
        }
        table.erase(key);
    }
    return total;
}

// @complexity time=O(n) space=O(1) conf=verified none=linear-op-in-loop,scan-in-loop scope=local
int hashed_set_ops(const std::vector<int>& values, std::unordered_set<int>& seen) {
    int total = 0;
    for (int value : values) {
        seen.insert(value);
        seen.emplace(value);
        if (seen.find(value) != seen.end() && seen.count(value) > 0 && seen.contains(value)) {
            total++;
        }
        seen.erase(value);
    }
    return total;
}

// @complexity time=O(n log m) space=O(1) conf=verified none=linear-op-in-loop,scan-in-loop scope=local
int heap_ops(const std::vector<int>& values, std::priority_queue<int>& queue) {
    int total = 0;
    for (int value : values) {
        queue.push(value);
        total += queue.top();
        queue.pop();
    }
    return total;
}

// @complexity time=O(n*m) space=O(1) conf=verified rule=scan-in-loop may=linear-op-in-loop scope=local
int string_searches(const std::vector<std::string>& words, const std::string& text) {
    int found = 0;
    for (const std::string& word : words) {
        found += static_cast<int>(text.find(word));
        found += static_cast<int>(text.rfind(word));
        found += static_cast<int>(text.find_first_of(word));
        found += text.compare(word);
    }
    return found;
}

// @complexity time=O(n*m) space=O(m) conf=verified rule=scan-in-loop,repeated-computation-in-loop may=linear-op-in-loop scope=local
int string_copies(const std::vector<std::string>& words, const std::string& text) {
    int size = 0;
    for (const std::string& word : words) {
        size += static_cast<int>(text.substr(1).size()) + static_cast<int>(word.size());
    }
    return size;
}

// @complexity time=O(n*m) space=O(1) conf=verified rule=linear-op-in-loop may=scan-in-loop scope=local
void string_edits(const std::vector<std::string>& words, std::string& text) {
    for (const std::string& word : words) {
        text.insert(0, word);
        text.erase(0, 1);
        text.replace(0, 1, word);
    }
}

// @complexity time=O(n) space=O(n) conf=verified none=linear-op-in-loop,scan-in-loop scope=local
void string_appends(const std::vector<std::string>& words, std::string& text) {
    for (const std::string& word : words) {
        text.append(word);
        text.push_back(',');
    }
}

// @complexity time=O(n*m) space=O(1) conf=verified rule=scan-in-loop may=linear-op-in-loop scope=local
int algorithm_searches(int rounds, const std::vector<int>& items) {
    int total = 0;
    for (int i = 0; i < rounds; i++) {
        total += *std::find(items.begin(), items.end(), i);
        total += static_cast<int>(std::count(items.begin(), items.end(), i));
        total += static_cast<int>(std::count_if(items.begin(), items.end(), [](int v) { return v > 0; }));
        total += *std::find_if(items.begin(), items.end(), [](int v) { return v > 1; });
        total += *std::max_element(items.begin(), items.end()) + *std::min_element(items.begin(), items.end());
        total += std::accumulate(items.begin(), items.end(), 0);
        if (std::all_of(items.begin(), items.end(), [](int v) { return v > 0; })
            || std::any_of(items.begin(), items.end(), [](int v) { return v > 1; })
            || std::none_of(items.begin(), items.end(), [](int v) { return v > 2; })
            || std::equal(items.begin(), items.end(), items.begin())) {
            total++;
        }
    }
    return total;
}

// @complexity time=O(n*m log m) space=O(1) conf=verified rule=scan-in-loop may=linear-op-in-loop scope=local
void algorithm_sorts(int rounds, std::vector<int>& items) {
    for (int i = 0; i < rounds; i++) {
        std::sort(items.begin(), items.end());
        std::stable_sort(items.begin(), items.end());
        std::partial_sort(items.begin(), items.begin() + 1, items.end());
    }
}

// @complexity time=O(n*m) space=O(1) conf=verified rule=linear-op-in-loop may=scan-in-loop scope=local
void algorithm_mutations(int rounds, std::vector<int>& items, std::vector<int>& out) {
    for (int i = 0; i < rounds; i++) {
        std::reverse(items.begin(), items.end());
        std::fill(items.begin(), items.end(), i);
        std::iota(items.begin(), items.end(), i);
        std::nth_element(items.begin(), items.begin() + 1, items.end());
        std::next_permutation(items.begin(), items.end());
        std::unique(items.begin(), items.end());
        std::remove(items.begin(), items.end(), i);
        std::remove_if(items.begin(), items.end(), [](int v) { return v < 0; });
        std::copy(items.begin(), items.end(), out.begin());
    }
}

// @complexity time=O(n log m) space=O(1) conf=verified none=linear-op-in-loop,scan-in-loop scope=local
int algorithm_ranks(const std::vector<int>& targets, const std::vector<int>& sorted) {
    int total = 0;
    for (int target : targets) {
        total += std::binary_search(sorted.begin(), sorted.end(), target) ? 1 : 0;
        total += static_cast<int>(std::lower_bound(sorted.begin(), sorted.end(), target) - sorted.begin());
        total += static_cast<int>(std::upper_bound(sorted.begin(), sorted.end(), target) - sorted.begin());
    }
    return total;
}

// @complexity time=O(n*m log m) space=O(1) conf=verified rule=linear-op-in-loop may=scan-in-loop scope=local
void heap_algorithms(int rounds, std::vector<int>& items) {
    for (int i = 0; i < rounds; i++) {
        std::make_heap(items.begin(), items.end());
        std::sort_heap(items.begin(), items.end());
    }
}

// @complexity time=O(n log m) space=O(1) conf=verified none=linear-op-in-loop,scan-in-loop scope=local
void heap_algorithm_churn(const std::vector<int>& values, std::vector<int>& items) {
    for (int value : values) {
        items.push_back(value);
        std::push_heap(items.begin(), items.end());
        std::pop_heap(items.begin(), items.end());
        items.pop_back();
    }
}

// @complexity time=O(n^3) space=O(1) conf=verified scope=local
int zero_triples(const std::vector<int>& values) {
    int hits = 0;
    for (size_t i = 0; i < values.size(); i++) {
        for (size_t j = i + 1; j < values.size(); j++) {
            for (size_t k = j + 1; k < values.size(); k++) {
                if (values[i] + values[j] + values[k] == 0) {
                    hits++;
                }
            }
        }
    }
    return hits;
}

// @complexity time=O(1) space=O(1) conf=verified scope=local
int multiplication_table() {
    int total = 0;
    for (int i = 0; i < 8; i++) {
        for (int j = 0; j < 8; j++) {
            total += i * j;
        }
    }
    return total;
}

// @complexity time=O(n^2) space=O(n) conf=verified rule=deep-recursion
int triangle_of_sums(int n) {
    if (n <= 0) {
        return 0;
    }
    int inner = 0;
    for (int i = 0; i < n; i++) {
        inner += i;
    }
    return inner + triangle_of_sums(n - 1);
}

// @complexity time=O(n) space=O(n) conf=verified none=branching-recursion
int climb_ways(int steps, std::unordered_map<int, int>& memo) {
    if (steps <= 1) {
        return 1;
    }
    auto known = memo.find(steps);
    if (known != memo.end()) {
        return known->second;
    }
    int ways = climb_ways(steps - 1, memo) + climb_ways(steps - 2, memo);
    memo[steps] = ways;
    return ways;
}
