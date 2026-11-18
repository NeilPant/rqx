// A stream is a loop per stage, and a lambda that scans adds a scan. `scope=local` marks
// the methods whose own time the per-file extraction is held to.
package corpus;

import java.util.List;
import java.util.stream.Collectors;

public final class Streams {
    // @complexity time=O(n) space=O(n) conf=verified scope=local
    public static List<Integer> evenSquares(List<Integer> values) {
        return values.stream().filter(v -> v % 2 == 0).map(v -> v * v).collect(Collectors.toList());
    }

    // @complexity time=O(n*m) space=O(1) conf=verified rule=scan-in-loop may=linear-op-in-loop scope=local
    public static boolean anyListed(List<Integer> values, List<Integer> wanted) {
        return values.stream().anyMatch(value -> wanted.contains(value));
    }

    // @complexity time=O(n) space=O(1) conf=verified
    public static int totalLength(List<String> words) {
        return words.stream().mapToInt(String::length).sum();
    }
}
