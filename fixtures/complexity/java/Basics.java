// Seed functions for the complexity precision corpus. Each annotation is the
// ground truth the inference is scored against; `space` counts the call stack.
package corpus;

import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Set;

public final class Basics {
    // @complexity time=O(n) space=O(1) conf=verified scope=local
    public static int sumAll(List<Integer> values) {
        int total = 0;
        for (int value : values) {
            total += value;
        }
        return total;
    }

    // @complexity time=O(n^2) space=O(n) conf=verified rule=linear-op-in-loop scope=local
    public static List<Integer> dedupeWithList(List<Integer> values) {
        List<Integer> unique = new ArrayList<>();
        for (int value : values) {
            if (!unique.contains(value)) {
                unique.add(value);
            }
        }
        return unique;
    }

    // @complexity time=O(n) space=O(n) conf=verified none=linear-op-in-loop scope=local
    public static List<Integer> dedupeWithSet(List<Integer> values) {
        Set<Integer> seen = new HashSet<>();
        List<Integer> unique = new ArrayList<>();
        for (int value : values) {
            if (seen.add(value)) {
                unique.add(value);
            }
        }
        return unique;
    }

    // @complexity time=O(log n) space=O(1) conf=verified scope=local
    public static int search(int[] sorted, int target) {
        int low = 0;
        int high = sorted.length - 1;
        while (low <= high) {
            int mid = (low + high) >>> 1;
            if (sorted[mid] == target) {
                return mid;
            }
            if (sorted[mid] < target) {
                low = mid + 1;
            } else {
                high = mid - 1;
            }
        }
        return -1;
    }
}
