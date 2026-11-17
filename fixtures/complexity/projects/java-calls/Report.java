package corpus;

import java.util.List;

public final class Report {
    // @complexity time=O(n^2) space=O(1) conf=verified
    public static int shares(List<Integer> values) {
        int largest = 0;
        for (int value : values) {
            largest = Math.max(largest, value * 100 / Math.max(1, Util.total(values)));
        }
        return largest;
    }
}
