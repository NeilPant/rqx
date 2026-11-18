#include "cost_repeated_computation_in_loop.hpp"

int total_length(const std::vector<std::string>& parts) {
  int total = 0;
  for (const std::string& part : parts) {
    total += static_cast<int>(part.size());
  }
  return total;
}

std::vector<double> shares(const std::vector<std::string>& parts, const std::vector<double>& weights) {
  std::vector<double> out;
  for (double weight : weights) {
    int whole = total_length(parts);
    out.push_back(weight / whole);
  }
  return out;
}

std::vector<double> shares_of_each(const std::vector<double>& weights) {
  std::vector<double> out;
  for (double weight : weights) {
    std::vector<std::string> own = {"x"};
    out.push_back(weight / total_length(own));
  }
  return out;
}
