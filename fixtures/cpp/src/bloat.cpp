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

class SameAsOvergrown : public Overgrown {};

class Formatting {
public:
    static int pad(int text) { return text; }
    static int trim(int text) { return text; }
};

class Catalogue {
public:
    Catalogue(int path) { rows = load_rows(path); }
    int size() { return this->rows; }
private:
    int rows;
};

int crowded_scope(int seed) {
    int c1 = seed; int c2 = seed; int c3 = seed; int c4 = seed;
    int c5 = seed; int c6 = seed; int c7 = seed; int c8 = seed;
    int c9 = seed; int c10 = seed; int c11 = seed; int c12 = seed;
    int c13 = seed; int c14 = seed; int c15 = seed; int c16 = seed;
    int all[] = {c1, c2, c3, c4, c5, c6, c7, c8, c9, c10, c11, c12, c13, c14, c15, c16};
    return all[0];
}

int widely_guarded(int seed) {
    try {
        int d1 = seed; int d2 = seed; int d3 = seed; int d4 = seed;
        int d5 = seed; int d6 = seed; int d7 = seed; int d8 = seed;
        int d9 = seed; int d10 = seed; int d11 = seed;
        int some[] = {d1, d2, d3, d4, d5, d6, d7, d8, d9, d10, d11};
        return some[0];
    } catch (const std::runtime_error& error) {
        return 0;
    }
}
