#include "cost_missing_capacity.hpp"

std::vector<std::string> copy_names(const std::vector<std::string>& names) {
  std::vector<std::string> copies;
  for (const std::string& name : names) {
    copies.push_back(name);
  }
  return copies;
}

std::vector<std::string> copy_names_sized(const std::vector<std::string>& names) {
  std::vector<std::string> copies;
  copies.reserve(names.size());
  for (const std::string& name : names) {
    copies.push_back(name);
  }
  return copies;
}
