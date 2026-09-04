#include "names.hpp"

int process(const std::vector<int>& rows) {
    std::vector<int> data;
    for (int row : rows) {
        if (row != 0) {
            data.push_back(row);
        }
    }
    int temp2 = static_cast<int>(data.size());
    return temp2;
}
