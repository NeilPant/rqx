#pragma once
#include <mutex>
#include <string>
#include <vector>

int parsed_code(const std::string& text);
std::size_t total_length(const std::vector<std::string>& words);
int locked_sum(std::mutex& guard, const std::vector<int>& rows);
