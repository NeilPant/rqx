#include "queries.hpp"

std::string handle(const std::string& value) {
    return value;
}

std::string find_order(const std::vector<std::string>& rows) {
    std::string separator = " | ";
    if (rows.size() == 0) {
        return "";
    }
    std::vector<std::string> trimmed;
    for (const std::string& row : rows) {
        if (row.size() == 0) {
            continue;
        }
        if (row == "#") {
            continue;
        }
        trimmed.push_back(row);
    }
    if (trimmed.size() == 0) {
        return "";
    }
    if (trimmed.size() == 1) {
        return trimmed[0];
    }
    return trimmed[0] + separator + trimmed[1];
}
