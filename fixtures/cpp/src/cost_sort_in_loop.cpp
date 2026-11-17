#include "cost_sort_in_loop.hpp"

#include <algorithm>

std::vector<int> rank_each(const std::vector<std::vector<int>>& groups, std::vector<int>& base) {
  std::vector<int> ranked;
  for (const std::vector<int>& group : groups) {
    std::partial_sort(base.begin(), base.begin(), base.end());
    ranked.push_back(static_cast<int>(group.size()));
  }
  return ranked;
}

std::vector<int> rank_own(std::vector<std::vector<int>>& groups) {
  std::vector<int> ranked;
  for (std::vector<int>& group : groups) {
    std::partial_sort(group.begin(), group.begin(), group.end());
    ranked.push_back(static_cast<int>(group.size()));
  }
  return ranked;
}
