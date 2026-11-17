// Library calls: a declared receiver is costed from the table at full confidence, and a
// hash-keyed collection is a lookup. `scope=local` marks the functions whose own time
// the per-file extraction is held to.
package corpus;

import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.HashSet;
import java.util.LinkedList;
import java.util.List;
import java.util.Map;
import java.util.Set;

public final class Library {
    // @complexity time=O(n*m) space=O(1) conf=verified rule=linear-op-in-loop scope=local
    public static int containsAny(List<Integer> values, List<Integer> wanted) {
        int hits = 0;
        for (int value : values) {
            if (wanted.contains(value)) {
                hits++;
            }
        }
        return hits;
    }

    // @complexity time=O(n) space=O(1) conf=verified none=linear-op-in-loop scope=local
    public static int containsAnyKeyed(List<Integer> values, Set<Integer> wanted) {
        int hits = 0;
        for (int value : values) {
            if (wanted.contains(value)) {
                hits++;
            }
        }
        return hits;
    }

    // @complexity time=O(n*m) space=O(1) conf=verified rule=linear-op-in-loop scope=local
    public static int linkedGets(LinkedList<Integer> chain, int count) {
        int total = 0;
        for (int i = 0; i < count; i++) {
            total += chain.get(i);
        }
        return total;
    }

    // @complexity time=O(n) space=O(1) conf=verified none=linear-op-in-loop scope=local
    public static int arrayGets(ArrayList<Integer> items, int count) {
        int total = 0;
        for (int i = 0; i < count; i++) {
            total += items.get(i);
        }
        return total;
    }

    // @complexity time=O(n) space=O(1) conf=verified none=linear-op-in-loop scope=local
    public static int lookupAll(Map<String, Integer> table, List<String> keys) {
        int total = 0;
        for (String key : keys) {
            total += table.getOrDefault(key, 0);
        }
        return total;
    }

    // @complexity time=O(n log n) space=O(1) conf=verified scope=local
    public static void sortInPlace(List<Integer> values) {
        Collections.sort(values);
    }

    // @complexity time=O(n*m) space=O(1) conf=verified scope=local
    public static int mentions(String text, List<String> words) {
        int hits = 0;
        for (String word : words) {
            if (text.contains(word)) {
                hits++;
            }
        }
        return hits;
    }

    // @complexity time=O(n) space=O(n) conf=verified scope=local
    public static List<Integer> dedupe(List<Integer> values) {
        Set<Integer> seen = new HashSet<>();
        List<Integer> unique = new ArrayList<>();
        for (int value : values) {
            if (seen.add(value)) {
                unique.add(value);
            }
        }
        return unique;
    }

    // @complexity time=O(n) space=O(n) conf=verified scope=local
    public static Map<String, Integer> index(List<String> words) {
        Map<String, Integer> index = new HashMap<>();
        for (String word : words) {
            index.put(word, word.length());
        }
        return index;
    }
}
