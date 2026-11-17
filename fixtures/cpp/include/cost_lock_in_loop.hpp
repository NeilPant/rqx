#pragma once

#include <mutex>
#include <vector>

int tally_under_one_lock(std::mutex& guard, const std::vector<int>& rows);
int tally_under_each_lock(std::vector<std::mutex>& guards, const std::vector<int>& rows);
