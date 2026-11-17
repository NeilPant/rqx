#include "cost_lock_in_loop.hpp"

int tally_under_one_lock(std::mutex& guard, const std::vector<int>& rows) {
  int total = 0;
  for (int row : rows) {
    std::lock_guard<std::mutex> hold(guard);
    total += row;
  }
  return total;
}

int tally_under_each_lock(std::vector<std::mutex>& guards, const std::vector<int>& rows) {
  int total = 0;
  for (int row : rows) {
    std::lock_guard<std::mutex> hold(guards[row]);
    total += row;
  }
  return total;
}
