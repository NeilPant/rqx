// Code that resembles a finding and is not one. Every rule should stay silent here.
#include "lookalikes.hpp"

// distant-declaration, short-name-wide-scope: `tz` belongs to the module, so being
// read at the bottom of the file is where it belongs.
const std::string tz = "UTC";

// output-parameter: `factor` in `counted` is a local that shares its name with
// this parameter, not this parameter.
int scaled(int factor) {
    return factor * 2;
}

int counted(int items) {
    int factor = items;
    factor += 1;
    return factor;
}

// mixed-language-file: an error message that names an update is not SQL.
std::string update_failure(const std::string& id) {
    return "Failed to update user " + id;
}

std::string zone_label() {
    return "zone " + tz;
}

// magic-number: each 30 below is the value of a declaration, which names it.
const int connect_timeout_seconds = 30;
const int read_timeout_seconds = 30;

int timeout_budget() {
    const int idle_timeout_seconds = 30;
    return connect_timeout_seconds + read_timeout_seconds + idle_timeout_seconds;
}

int level(int value) {
    return value < 0 ? 0 : value;
}
