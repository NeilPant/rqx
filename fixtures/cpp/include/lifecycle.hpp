#pragma once
#include <string>
#include <vector>

class Session {
private:
    std::string socket;
    bool opened = false;
public:
    void open(const std::string& target) {
        this->socket = target;
        this->opened = true;
    }

    std::string send(const std::string& payload) {
        if (!this->opened) {
            return "";
        }
        return this->socket + ":" + payload;
    }

    void close() {
        if (this->opened) {
            this->socket = "";
        }
    }
};

class ReportFactory {
private:
    std::vector<std::string> rows;
public:
    void add(const std::string& row) { this->rows.push_back(row); }
};
