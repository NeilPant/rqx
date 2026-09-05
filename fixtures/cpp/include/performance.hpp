#pragma once

#include <string>
#include <vector>

struct RowStore {
  std::vector<std::string> values;

  std::string find_by_id(int id) const {
    if (id < static_cast<int>(values.size())) {
      return values[id];
    }
    return "";
  }

  int count() const { return static_cast<int>(values.size()); }
};

struct LineTask {
  std::string value;

  bool ready() const { return !value.empty(); }

  LineTask wait(const std::string& path) const { return LineTask{path}; }
};

LineTask read_each(const std::vector<std::string>& paths);
std::vector<int> keep_wanted(const std::vector<int>& rows, const std::vector<int>& wanted);
int sum_cube(const std::vector<std::vector<std::vector<int>>>& grid);
std::vector<std::string> query_each(const std::vector<int>& ids);
int count_digits(const std::vector<std::string>& lines);
std::string join_by_copy(const std::vector<std::string>& parts);
int fibonacci(int step);
