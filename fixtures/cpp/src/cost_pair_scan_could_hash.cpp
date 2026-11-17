#include "cost_pair_scan_could_hash.hpp"

std::vector<std::string> owners_of(const std::vector<Order>& orders, const std::vector<User>& users) {
  std::vector<std::string> names;
  for (const Order& order : orders) {
    for (const User& user : users) {
      if (order.user_id == user.id) {
        names.push_back(user.name);
      }
    }
  }
  return names;
}

int same_order(const std::vector<Order>& orders) {
  int pairs = 0;
  for (const Order& left : orders) {
    for (const Order& right : orders) {
      if (left.id == right.user_id) {
        pairs += 1;
      }
    }
  }
  return pairs;
}
