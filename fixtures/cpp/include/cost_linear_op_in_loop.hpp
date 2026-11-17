#pragma once

#include <deque>
#include <vector>

void prepend_each(std::vector<int>& out, const std::vector<int>& items);
void front_each(std::deque<int>& out, const std::vector<int>& items);
