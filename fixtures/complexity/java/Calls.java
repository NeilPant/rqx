// A function that calls another one, in the cases the cost rules tell apart.
package corpus;

import java.util.ArrayList;
import java.util.List;

public final class Calls {
    // @complexity time=O(n) space=O(1) conf=verified scope=local
    static int indexOfItem(List<String> list, String wanted) {
        for (int i = 0; i < list.size(); i++) {
            if (list.get(i).equals(wanted)) {
                return i;
            }
        }
        return -1;
    }

    // @complexity time=O(n^2) space=O(n) conf=verified rule=quadratic-via-call
    public static List<String> firstOccurrences(List<String> items) {
        List<String> firsts = new ArrayList<>();
        for (int at = 0; at < items.size(); at++) {
            if (indexOfItem(items, items.get(at)) == at) {
                firsts.add(items.get(at));
            }
        }
        return firsts;
    }

    // @complexity time=O(1) space=O(1) conf=verified scope=local
    static int scale(int x) {
        return x * 2;
    }

    // @complexity time=O(n) space=O(n) conf=verified none=quadratic-via-call
    public static List<Integer> scaleEach(List<Integer> items) {
        List<Integer> out = new ArrayList<>();
        for (int item : items) {
            out.add(scale(item));
        }
        return out;
    }

    // @complexity time=O(n) space=O(1) conf=verified scope=local
    static int totalLength(List<String> parts) {
        int total = 0;
        for (String part : parts) {
            total += part.length();
        }
        return total;
    }

    // @complexity time=O(n*m) space=O(m) conf=verified rule=repeated-computation-in-loop
    public static List<Double> shares(List<String> parts, List<Double> weights) {
        List<Double> out = new ArrayList<>();
        for (double weight : weights) {
            int whole = totalLength(parts);
            out.add(weight / whole);
        }
        return out;
    }

    // @complexity time=O(n*m) space=O(n) conf=verified none=repeated-computation-in-loop
    public static List<Integer> lengthOfEach(List<List<String>> groups) {
        List<Integer> out = new ArrayList<>();
        for (List<String> group : groups) {
            out.add(totalLength(group));
        }
        return out;
    }

    // @complexity time=O(n) space=O(n) conf=verified rule=deep-recursion
    public static int sumTo(int n) {
        if (n <= 0) {
            return 0;
        }
        return n + sumTo(n - 1);
    }

    // @complexity time=O(log n) space=O(log n) conf=verified none=deep-recursion
    public static int bits(int n) {
        if (n <= 1) {
            return 0;
        }
        return 1 + bits(n >> 1);
    }

    // @complexity time=O(n^2) space=O(1) conf=verified scope=local none=pair-scan-could-hash
    public static int countPairs(List<Integer> items) {
        int pairs = 0;
        for (int left : items) {
            for (int right : items) {
                if (left == right) {
                    pairs += 1;
                }
            }
        }
        return pairs;
    }
}
