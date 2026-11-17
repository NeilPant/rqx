#pragma once

#include <string>
#include <vector>

std::size_t count_characters(std::vector<std::string> lines);
std::size_t count_characters_by_reference(const std::vector<std::string>& lines);
std::string shouted(std::string text);

class Collected {
 public:
  explicit Collected(std::vector<std::string> lines) : lines_(std::move(lines)) {}
  std::size_t size() const { return lines_.size(); }

 private:
  std::vector<std::string> lines_;
};
