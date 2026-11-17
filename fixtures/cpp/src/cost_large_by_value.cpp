#include "cost_large_by_value.hpp"

std::size_t count_characters(std::vector<std::string> lines) {
  std::size_t total = 0;
  for (const std::string& line : lines) {
    total += line.size();
  }
  return total;
}

std::size_t count_characters_by_reference(const std::vector<std::string>& lines) {
  std::size_t total = 0;
  for (const std::string& line : lines) {
    total += line.size();
  }
  return total;
}

std::string shouted(std::string text) {
  text += "!";
  return text;
}
