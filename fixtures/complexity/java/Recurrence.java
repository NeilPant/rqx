// Recursion, one shape each. Their cost needs the call graph, so none is marked local.
package corpus;

import java.util.HashMap;
import java.util.List;
import java.util.Map;

public final class Recurrence {
    // @complexity time=O(n) space=O(n) conf=verified rule=deep-recursion
    public static long factorial(int n) {
        return n <= 1 ? 1 : n * factorial(n - 1);
    }

    // @complexity time=O(log n) space=O(log n) conf=verified
    public static long power(long base, int exponent) {
        if (exponent == 0) {
            return 1;
        }
        long half = power(base, exponent / 2);
        return exponent % 2 == 0 ? half * half : half * half * base;
    }

    // @complexity time=O(n) space=O(n) conf=verified rule=deep-recursion
    public static int sumTail(List<Integer> values) {
        if (values.isEmpty()) {
            return 0;
        }
        return values.get(0) + sumTail(values.subList(1, values.size()));
    }

    // @complexity time=exp space=O(n) conf=verified rule=branching-recursion
    public static long paths(int n) {
        if (n <= 1) {
            return 1;
        }
        return paths(n - 1) + paths(n - 2) + paths(n - 3);
    }

    // @complexity time=O(n) space=O(n) conf=verified none=branching-recursion
    public static long pathsMemo(int n, Map<Integer, Long> memo) {
        if (n <= 1) {
            return 1;
        }
        Long cached = memo.get(n);
        if (cached != null) {
            return cached;
        }
        long value = pathsMemo(n - 1, memo) + pathsMemo(n - 2, memo);
        memo.put(n, value);
        return value;
    }
}
