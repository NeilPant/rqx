// Three levels of calls in one file: the cost of the top is the loop it runs times what
// the levels below cost on each pass. `scope=local` marks the functions whose own time
// the per-file extraction is held to.
#include <algorithm>
#include <vector>

// @complexity time=O(n) space=O(1) conf=verified scope=local
int row_total(const std::vector<int>& row) {
    int total = 0;
    for (int cell : row) {
        total += cell;
    }
    return total;
}

// @complexity time=O(n) space=O(1) conf=verified
double row_summary(const std::vector<int>& row) {
    return static_cast<double>(row_total(row)) / std::max<size_t>(1, row.size());
}

// @complexity time=O(n*m) space=O(n) conf=verified
std::vector<double> row_report(const std::vector<std::vector<int>>& rows) {
    std::vector<double> summaries;
    for (const auto& row : rows) {
        summaries.push_back(row_summary(row));
    }
    return summaries;
}
