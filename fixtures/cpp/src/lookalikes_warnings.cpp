// Code that resembles a warning and is not one. Every rule should stay silent here.
#include "lookalikes_warnings.hpp"

// catch-all-handler: `catch (const char*)` names the one type it takes; only
// `catch (...)` takes everything.
static int checked_size(const std::string& text) {
    if (text.empty()) {
        throw "empty";
    }
    return static_cast<int>(text.size());
}

int parsed_code(const std::string& text) {
    try {
        return checked_size(text);
    } catch (const char*) {
        return -1;
    }
}

// imprecise-type: `auto` is deduced from what it is given, so it is exactly that
// type, and `(void)` says a value is not used rather than converting it.
std::size_t total_length(const std::vector<std::string>& words) {
    std::size_t total = 0;
    for (const auto& word : words) {
        total += word.size();
    }
    const auto count = words.size();
    (void)count;
    return total;
}

// unused-parameter: both parameters are read. Each is named inside a reference
// declarator, and `hold(guard)` reads `guard` although the grammar takes it for a
// type.
int locked_sum(std::mutex& guard, const std::vector<int>& rows) {
    std::lock_guard<std::mutex> hold(guard);
    int sum = 0;
    for (const int row : rows) {
        sum += row;
    }
    return sum;
}

// dead-store: each write sits in its own preprocessor branch, and only one of them
// is compiled.
int platform_code() {
#ifdef _WIN32
    const int code = 1;
#else
    const int code = 2;
#endif
    return code;
}
