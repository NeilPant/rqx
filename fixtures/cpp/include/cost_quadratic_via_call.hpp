#pragma once

#include <string>
#include <vector>

int index_of_item(const std::vector<std::string>& list, const std::string& wanted);
std::vector<std::string> first_occurrences(const std::vector<std::string>& items);
std::vector<std::string> missing_from(const std::vector<std::string>& items,
                                      const std::vector<std::string>& known);
