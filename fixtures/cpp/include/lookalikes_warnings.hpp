#pragma once
#include <mutex>
#include <string>
#include <vector>

int parsed_code(const std::string& text);
std::size_t total_length(const std::vector<std::string>& words);
int locked_sum(std::mutex& guard, const std::vector<int>& rows);
int platform_code();

// unused-private-member: a member function reads the field by its bare name.
class Role {
  public:
    explicit Role(int line) : line_(line) {}
    int line() const { return line_; }

  private:
    int line_;
};
