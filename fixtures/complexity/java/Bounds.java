// Loop bounds, one class each: a fixed number, the size of a collection or a count, a
// logarithm, and the shapes that must not change the answer. `scope=local` marks the
// functions whose own time the per-file extraction is held to.
package corpus;

import java.util.List;
import java.util.Map;

public final class Bounds {
    private static final int WINDOW = 8;

    // @complexity time=O(1) space=O(1) conf=verified scope=local
    public static int windowSum(int[] values) {
        int total = 0;
        for (int i = 0; i < WINDOW; i++) {
            total += values[i];
        }
        return total;
    }

    // @complexity time=O(n) space=O(1) conf=verified scope=local
    public static int countUp(int n) {
        int total = 0;
        for (int i = 0; i < n; i++) {
            total += i;
        }
        return total;
    }

    // @complexity time=O(n) space=O(1) conf=verified scope=local
    public static int sumArray(int[] values) {
        int total = 0;
        for (int i = 0; i < values.length; i++) {
            total += values[i];
        }
        return total;
    }

    // @complexity time=O(log n) space=O(1) conf=verified scope=local
    public static int halvings(int n) {
        int steps = 0;
        while (n > 1) {
            n = n / 2;
            steps++;
        }
        return steps;
    }

    // @complexity time=O(log n) space=O(1) conf=verified scope=local
    public static int powersBelow(int limit) {
        int count = 0;
        for (int p = 1; p < limit; p *= 2) {
            count++;
        }
        return count;
    }

    // @complexity time=O(n^2) space=O(1) conf=verified scope=local
    public static int countInversions(List<Integer> values) {
        int inversions = 0;
        for (int i = 0; i < values.size(); i++) {
            for (int j = i + 1; j < values.size(); j++) {
                if (values.get(i) > values.get(j)) {
                    inversions++;
                }
            }
        }
        return inversions;
    }

    // @complexity time=O(n) space=O(1) conf=verified scope=local
    public static Integer firstNegative(List<Integer> values) {
        for (int value : values) {
            if (value < 0) {
                return value;
            }
        }
        return null;
    }

    // @complexity time=O(log n) space=O(1) conf=verified scope=local
    public static int lowerBound(int[] sorted, int target) {
        int low = 0;
        int high = sorted.length;
        while (low < high) {
            int mid = (low + high) >>> 1;
            if (sorted[mid] < target) {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        return low;
    }

    // @complexity time=O(n) space=O(1) conf=verified scope=local
    public static int sumValues(Map<String, Integer> table) {
        int total = 0;
        for (int value : table.values()) {
            total += value;
        }
        return total;
    }

    // @complexity time=O(n*m) space=O(1) conf=verified scope=local
    public static int everyPair(List<String> left, List<String> right) {
        int[] pairs = {0};
        left.forEach(a -> right.forEach(b -> pairs[0] += a.length() + b.length()));
        return pairs[0];
    }
}
