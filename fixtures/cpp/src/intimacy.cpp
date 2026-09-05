#include "intimacy.hpp"

int Checkout::rate = 3;

class Ledger {
public:
    Ledger() { this->writer = new CsvWriter(); }
    int stamped_at() { return static_cast<int>(time(nullptr)); }
private:
    CsvWriter* writer;
};
