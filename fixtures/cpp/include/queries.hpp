#pragma once
#include <string>
#include <vector>

class Ledger {
private:
    int total = 0;
    int reads = 0;
public:
    int getTotal() {
        this->reads += 1;
        return this->total;
    }
};

std::string handle(const std::string& value);
std::string find_order(const std::vector<std::string>& rows);
