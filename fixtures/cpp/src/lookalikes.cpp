// Code that resembles a finding and is not one. Every rule should stay silent here.
#include "lookalikes.hpp"

#include <algorithm>
#include <vector>

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

// empty-catch: a catch that says why it does nothing has handled the error.
int parsed_or_zero(const std::string& text) {
    try {
        return std::stoi(text);
    } catch (const std::invalid_argument&) {
        // Malformed input counts as zero, which every caller expects.
    }
    return 0;
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

// distant-declaration: the table is read while it is declared, and used on the next line.
int spelled(const std::string& word) {
    const std::vector<std::string> code_words = {
        "alfa",
        "bravo",
        "charlie",
        "delta",
        "echo",
        "foxtrot",
        "golf",
        "hotel",
        "india",
        "juliett",
        "kilo",
        "lima",
        "mike",
        "november",
        "oscar",
        "papa",
        "quebec",
        "romeo",
        "sierra",
        "tango",
        "uniform",
        "victor",
        "whiskey",
        "xray",
        "yankee",
        "zulu",
    };
    return static_cast<int>(std::count(code_words.begin(), code_words.end(), word));
}

// never-reassigned: a range-for variable already declared const.
std::size_t longest(const std::vector<std::string>& words) {
    std::size_t longest = 0;
    for (const std::string& word : words) {
        longest = std::max(longest, word.size());
    }
    return longest;
}

// unreachable-code: the label after the return is where the goto lands, so the code
// under it runs.
int release_on_failure(int* handle, int status) {
    if (status != 0) goto cleanup;
    return 0;
cleanup:
    delete handle;
    return status;
}

// unreachable-code: a macro can expand to anything, so a call spelled as one after a
// return is not known to be dead.
int squared(int value) {
    return value * value;
    NOT_REACHED();
}

int parsed_or_zero(const std::string& text) {
    // shadowed-handler: a handler for a thrown string takes nothing the one below it takes.
    try {
        return std::stoi(text);
    } catch (const std::string& message) {
        return static_cast<int>(message.size());
    } catch (const std::out_of_range& error) {
        return static_cast<int>(text.size());
    }
}

bool is_missing(double reading) {
    // self-comparison: only NaN differs from itself, so this is how NaN is detected.
    return reading != reading;
}

int width(const std::string& value) {
    return static_cast<int>(value.size());
}

int width(long value) {
    // no-base-case: the call converts its argument, so it reaches the other overload.
    return width(std::to_string(value));
}
