#include "bloat.hpp"

int too_many_parameters(int a, int b, int c, int d, int e) {
    return a + b + c + d + e;
}

int deeply_nested(const std::vector<int>& rows) {
    for (int row : rows) {
        if (row > 0) {
            while (row < 10) {
                if (row == 5) {
                    return row;
                }
            }
        }
    }
    return 0;
}
