// Three levels of calls in one class: the cost of the top is the loop it runs times what
// the levels below cost on each pass. `scope=local` marks the methods whose own time the
// per-file extraction is held to.
package corpus;

import java.util.ArrayList;
import java.util.List;

public final class Chains {
    // @complexity time=O(n) space=O(1) conf=verified scope=local
    public static int rowTotal(List<Integer> row) {
        int total = 0;
        for (int cell : row) {
            total += cell;
        }
        return total;
    }

    // @complexity time=O(n) space=O(1) conf=verified
    public static double rowSummary(List<Integer> row) {
        return (double) rowTotal(row) / Math.max(1, row.size());
    }

    // @complexity time=O(n*m) space=O(n) conf=verified
    public static List<Double> rowReport(List<List<Integer>> rows) {
        List<Double> summaries = new ArrayList<>();
        for (List<Integer> row : rows) {
            summaries.add(rowSummary(row));
        }
        return summaries;
    }
}
