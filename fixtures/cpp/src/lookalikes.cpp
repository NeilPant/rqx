// Code that resembles a finding and is not one. Every rule should stay silent here.
#include "lookalikes.hpp"

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
