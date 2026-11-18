#include "cost_deep_recursion.hpp"

int sum_to(int n) {
  if (n <= 0) {
    return 0;
  }
  return n + sum_to(n - 1);
}

int bits(int n) {
  if (n <= 1) {
    return 0;
  }
  return 1 + bits(n >> 1);
}
