#pragma once

#include <string>
#include <vector>

struct Order {
  int id;
  int user_id;
};

struct User {
  int id;
  std::string name;
};

std::vector<std::string> owners_of(const std::vector<Order>& orders, const std::vector<User>& users);
int same_order(const std::vector<Order>& orders);
