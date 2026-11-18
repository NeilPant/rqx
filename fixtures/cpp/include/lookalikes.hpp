#pragma once
#include <string>

int scaled(int factor);
int counted(int items);
std::string update_failure(const std::string& id);
std::string zone_label();
int timeout_budget();
int level(int value);

// no-base-case: `::level` is the free function, not the member calling itself.
struct Gauge {
    int scale = 1;
    int level(int value) const { return ::level(value) * scale; }
};
