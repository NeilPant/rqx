#include "conventions.hpp"


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

int item_list[] = {1, 2, 3};

int narrates(int rows) {
    log.info("starting");
    int total = rows;
    log.info("counted");
    log.info("finishing");
    return total;
}
