#pragma once

#include <string>
#include <vector>

int total_length(const std::vector<std::string>& parts);
std::vector<double> shares(const std::vector<std::string>& parts, const std::vector<double>& weights);
std::vector<double> shares_of_each(const std::vector<double>& weights);
