#include "conventions.hpp"

int retry_limit = 3;
int retryBackoff = 250;

std::string query(const std::string& user_id) {
    return "SELECT id, name FROM users WHERE id = '" + user_id + "'";
}

std::vector<std::string> window(const std::vector<std::string>& items, int cursor) {
    std::vector<std::string> out;
    out.push_back(items[cursor + 1]);
    out.push_back(items[cursor + 1]);
    out.push_back(items[cursor + 1]);
    return out;
}
