#include "cost_copy_in_range_for.hpp"

std::size_t total_length_by_copy(const std::vector<std::string>& names) {
  std::size_t total = 0;
  for (std::string name : names) {
    total += name.size();
  }
  return total;
}

std::size_t total_length_by_reference(const std::vector<std::string>& names) {
  std::size_t total = 0;
  for (const std::string& name : names) {
    total += name.size();
  }
  return total;
}

std::size_t total_length_of_edited(const std::vector<std::string>& names) {
  std::size_t total = 0;
  for (std::string name : names) {
    name.push_back('!');
    total += name.size();
  }
  return total;
}
