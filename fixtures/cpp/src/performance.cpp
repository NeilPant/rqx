#include "performance.hpp"

#include <algorithm>
#include <regex>

std::vector<int> keep_wanted(const std::vector<int>& rows, const std::vector<int>& wanted) {
  std::vector<int> kept;
  for (int row : rows) {
    if (std::find(wanted.begin(), wanted.end(), row) != wanted.end()) {
      kept.push_back(row);
    }
  }
  return kept;
}

int sum_cube(const std::vector<std::vector<std::vector<int>>>& grid) {
  int total = 0;
  for (const std::vector<std::vector<int>>& plane : grid) {
    for (const std::vector<int>& line : plane) {
      for (int cell : line) {
        total += cell;
      }
    }
  }
  return total;
}

namespace {
const RowStore repository;
}  // namespace

LineTask read_each(const std::vector<std::string>& paths) {
  LineTask task;
  for (const std::string& path : paths) {
    task = co_await task.wait(path);
  }
  co_return task;
}

std::vector<std::string> query_each(const std::vector<int>& ids) {
  std::vector<std::string> found;
  for (int id : ids) {
    found.push_back(repository.find_by_id(id));
  }
  return found;
}

int count_digits(const std::vector<std::string>& lines) {
  int hits = 0;
  for (const std::string& line : lines) {
    std::regex digits("^[0-9]+$");
    hits += std::regex_match(line, digits) ? 1 : 0;
  }
  return hits;
}

std::string join_by_copy(const std::vector<std::string>& parts) {
  std::string text;
  for (const std::string& part : parts) {
    text += part;
  }
  return text;
}

int fibonacci(int step) {
  if (step < 2) {
    return step;
  }
  return fibonacci(step - 1) + fibonacci(step - 2);
}

std::vector<std::function<int()>> schedules(const std::vector<int>& rows) {
    std::vector<std::function<int()>> out;
    for (int at = 0; at < static_cast<int>(rows.size()); at++) {
        out.push_back([&]() { return at; });
    }
    return out;
}

int prunes(std::vector<int>& rows) {
    for (int row : rows) {
        if (row < 0) {
            rows.pop_back();
        }
    }
    return static_cast<int>(rows.size());
}

int guards(int value) {
    assert(value > 0);
    return value;
}

int open_tabs = 0;

void open_tab() {
    open_tabs += 1;
}

int tabs_open() {
    return open_tabs;
}

int paced() {
    sleep(500);
    return open_tabs;
}
