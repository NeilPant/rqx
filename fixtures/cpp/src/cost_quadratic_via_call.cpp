#include "cost_quadratic_via_call.hpp"

int index_of_item(const std::vector<std::string>& list, const std::string& wanted) {
  for (size_t i = 0; i < list.size(); i++) {
    if (list[i] == wanted) {
      return static_cast<int>(i);
    }
  }
  return -1;
}

std::vector<std::string> first_occurrences(const std::vector<std::string>& items) {
  std::vector<std::string> firsts;
  for (size_t at = 0; at < items.size(); at++) {
    if (index_of_item(items, items[at]) == static_cast<int>(at)) {
      firsts.push_back(items[at]);
    }
  }
  return firsts;
}

std::string describe(const std::string& text) {
  return text.substr(0, 8);
}

std::vector<std::string> describe_each(const std::vector<std::string>& items) {
  std::vector<std::string> out;
  for (const std::string& item : items) {
    out.push_back(describe(item));
  }
  return out;
}
