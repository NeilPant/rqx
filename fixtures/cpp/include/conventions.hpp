#pragma once
#include <string>
#include <vector>

extern int retry_limit;
extern int retryBackoff;

std::string query(const std::string& user_id);
std::vector<std::string> window(const std::vector<std::string>& items, int cursor);
