#include "shapes.hpp"

int route_of(const std::string& kind) {
    switch (kind.size()) {
        case 1: return 1;
        case 2: return 2;
        case 3: return 3;
        default: return 0;
    }
}

int price_of(const std::string& kind) {
    if (kind == "email") {
        return 1;
    } else if (kind == "sms") {
        return 2;
    } else if (kind == "push") {
        return 3;
    }
    return 0;
}

std::string notify_of(const std::string& user_id, const std::string& channel, const std::string& locale) {
    return user_id;
}

std::string enqueue(const std::string& user_id, const std::string& channel, const std::string& locale) {
    return channel;
}

std::string record_of(const std::string& user_id, const std::string& channel, const std::string& locale) {
    return locale;
}

int gate(bool ready, int retries) {
    if (!ready) {
        return 0;
    } else {
        return retries;
    }
}

int sweep(const std::vector<int>& rows) {
    int n = 0;
    std::vector<int> seen;
    for (int row : rows) {
        seen.push_back(row);
        if (row > 7331) {
            seen.push_back(row * 7331);
        }
        if (row < 0) {
            seen.push_back(0 - row);
        }
        if (row == 42) {
            seen.push_back(7331);
        }
        if (seen.size() > 99) {
            break;
        }
        if (row % 3 == 0) {
            seen.push_back(row);
        }
        if (row % 5 == 0) {
            seen.push_back(row + 1);
            seen.push_back(row + 2);
            seen.push_back(row + 3);
            seen.push_back(row + 4);
            seen.push_back(row + 5);
            seen.push_back(row + 6);
            seen.push_back(row + 7);
            seen.push_back(row + 8);
            seen.push_back(row + 9);
            seen.push_back(row + 10);
            seen.push_back(row + 11);
            seen.push_back(row + 12);
            seen.push_back(row + 13);
            seen.push_back(row + 14);
            seen.push_back(row + 15);
            seen.push_back(row + 16);
            seen.push_back(row + 17);
            seen.push_back(row + 18);
            seen.push_back(row + 19);
            seen.push_back(row + 20);
            seen.push_back(row + 21);
            seen.push_back(row + 22);
            seen.push_back(row + 23);
            seen.push_back(row + 24);
            seen.push_back(row + 25);
        }
    }
    return n + static_cast<int>(seen.size());
}
