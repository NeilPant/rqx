#include "cost_allocation_in_loop.hpp"

int buffer_each_row(const std::vector<int>& rows) {
  int total = 0;
  for (int row : rows) {
    std::vector<char> buffer(65536);
    total += static_cast<int>(buffer.size()) + row;
  }
  return total;
}

int pad_each_row(const std::vector<int>& rows) {
  int total = 0;
  for (int row : rows) {
    std::vector<char> padding(16);
    total += static_cast<int>(padding.size()) + row;
  }
  return total;
}
