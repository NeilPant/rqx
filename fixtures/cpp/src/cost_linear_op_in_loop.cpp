#include "cost_linear_op_in_loop.hpp"

void prepend_each(std::vector<int>& out, const std::vector<int>& items) {
  for (int item : items) {
    out.insert(out.begin(), item);
  }
}

void front_each(std::deque<int>& out, const std::vector<int>& items) {
  for (int item : items) {
    out.push_front(item);
  }
}
