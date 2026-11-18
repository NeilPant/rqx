#pragma once
#include <functional>
#include <string>

int with_dead_tail(int used, const std::string& dead);
int with_dead_middle(const std::string& dead, int used);
int respects_underscore(int used, const std::string& _deliberate);
int two_dead_on_one_line(int used, const std::string& deadA, const std::string& deadB);
auto adder(int step) -> std::function<int(int value)>;
