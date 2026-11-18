#pragma once
#include <string>
#include <vector>

int scaled(int factor);
int counted(int items);
std::string update_failure(const std::string& id);
std::string zone_label();
int timeout_budget();
int level(int value);
int spelled(const std::string& word);
std::size_t longest(const std::vector<std::string>& words);
int parsed_or_zero(const std::string& text);

// no-base-case: `::level` is the free function, not the member calling itself.
// empty-method: the constructor's work is its member initialiser list.
struct Meter {
    explicit Meter(int factor) : factor_(factor) {}
    int scaled(int value) const { return value * factor_; }
    int factor_;
};

struct Gauge {
    int scale = 1;
    int level(int value) const { return ::level(value) * scale; }
};
