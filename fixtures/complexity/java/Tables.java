// A tour of the library cost tables. Every method loops and calls methods of one
// collection, so what the table says a call costs is what the loop costs per pass.
// `scope=local` marks the methods whose own time the per-file extraction is held to.
package corpus;

import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.HashMap;
import java.util.HashSet;
import java.util.LinkedList;
import java.util.List;
import java.util.Map;
import java.util.PriorityQueue;
import java.util.TreeMap;
import java.util.TreeSet;

public final class Tables {
    // @complexity time=O(n*m) space=O(1) conf=verified rule=scan-in-loop may=linear-op-in-loop scope=local
    public static int listSearches(List<Integer> values, List<Integer> items) {
        int found = 0;
        for (int value : values) {
            if (items.contains(value)) {
                found++;
            }
            found += items.indexOf(value);
            found += items.lastIndexOf(value);
        }
        return found;
    }

    // @complexity time=O(n*m*k) space=O(1) conf=verified rule=scan-in-loop may=linear-op-in-loop scope=local
    public static int listBulkChecks(List<Integer> values, List<Integer> items, List<Integer> others) {
        int found = 0;
        for (int value : values) {
            if (items.containsAll(others)) {
                found += value;
            }
        }
        return found;
    }

    // @complexity time=O(n*m*k) space=O(1) conf=verified rule=linear-op-in-loop may=scan-in-loop scope=local
    public static void listBulkEdits(List<Integer> values, List<Integer> items, List<Integer> others) {
        for (int value : values) {
            items.removeAll(others);
            items.retainAll(others);
        }
    }

    // @complexity time=O(n*m) space=O(1) conf=verified rule=linear-op-in-loop may=scan-in-loop scope=local
    public static void listEdits(List<Integer> values, List<Integer> items) {
        for (int value : values) {
            items.add(0, value);
            items.remove(Integer.valueOf(value));
        }
    }

    // @complexity time=O(n) space=O(n) conf=verified none=linear-op-in-loop,scan-in-loop scope=local
    public static int listConstantOps(List<Integer> values, List<Integer> items) {
        int total = 0;
        for (int value : values) {
            items.add(value);
            items.set(0, value);
            total += items.get(0) + items.size() + items.subList(0, 1).size();
        }
        return total;
    }

    // @complexity time=O(n*m) space=O(m) conf=verified rule=scan-in-loop,repeated-computation-in-loop none=accumulate-by-copy may=linear-op-in-loop scope=local
    public static int listCopies(List<Integer> values, List<Integer> items) {
        int size = 0;
        for (int value : values) {
            size += items.toArray().length;
            size += List.copyOf(items).size() + value;
        }
        return size;
    }

    // @complexity time=O(n*m log m) space=O(1) conf=verified rule=scan-in-loop may=linear-op-in-loop scope=local
    public static void listSorts(int rounds, List<Integer> items) {
        for (int i = 0; i < rounds; i++) {
            items.sort(null);
            Collections.sort(items);
            Collections.reverse(items);
            Collections.shuffle(items);
        }
    }

    // @complexity time=O(n*m) space=O(1) conf=verified rule=scan-in-loop,repeated-computation-in-loop may=linear-op-in-loop scope=local
    public static int listExtremes(int rounds, List<Integer> items) {
        int total = 0;
        for (int i = 0; i < rounds; i++) {
            total += Collections.max(items) + Collections.min(items);
            total += Collections.frequency(items, 1);
        }
        return total;
    }

    // @complexity time=O(n*k) space=O(1) conf=verified rule=linear-op-in-loop may=scan-in-loop scope=local
    public static void listAddAll(List<Integer> values, List<Integer> items, List<Integer> more) {
        for (int value : values) {
            items.addAll(more);
        }
    }

    // @complexity time=O(n*m) space=O(1) conf=verified rule=linear-op-in-loop may=scan-in-loop scope=local
    public static void listClears(int rounds, List<Integer> items) {
        for (int i = 0; i < rounds; i++) {
            items.clear();
        }
    }

    // @complexity time=O(n*m) space=O(1) conf=verified rule=linear-op-in-loop may=scan-in-loop scope=local
    public static int linkedWalks(int count, LinkedList<Integer> chain) {
        int total = 0;
        for (int i = 0; i < count; i++) {
            chain.set(i, i);
            chain.add(i, i);
            total += chain.get(i);
        }
        return total;
    }

    // @complexity time=O(n) space=O(1) conf=verified none=linear-op-in-loop,scan-in-loop scope=local
    public static int dequeEnds(List<Integer> values, ArrayDeque<Integer> queue) {
        int total = 0;
        for (int value : values) {
            queue.addFirst(value);
            queue.addLast(value);
            queue.offer(value);
            total += queue.peek() + queue.poll();
            queue.poll();
            queue.poll();
        }
        return total;
    }

    // @complexity time=O(n*m) space=O(1) conf=verified rule=scan-in-loop may=linear-op-in-loop scope=local
    public static int dequeMembership(List<Integer> values, ArrayDeque<Integer> queue) {
        int found = 0;
        for (int value : values) {
            if (queue.contains(value)) {
                found++;
            }
        }
        return found;
    }

    // @complexity time=O(n) space=O(n) conf=verified none=linear-op-in-loop,scan-in-loop scope=local
    public static int mapOperations(List<String> keys, HashMap<String, Integer> table) {
        int total = 0;
        for (String key : keys) {
            table.put(key, 1);
            total += table.get(key) + table.getOrDefault(key, 0);
            if (table.containsKey(key)) {
                table.remove(key);
            }
            table.computeIfAbsent(key, k -> 1);
            table.merge(key, 1, Integer::sum);
            total += table.size() + table.keySet().size() + table.values().size() + table.entrySet().size();
        }
        return total;
    }

    // @complexity time=O(n*m) space=O(1) conf=verified rule=scan-in-loop may=linear-op-in-loop scope=local
    public static int mapValueScans(List<Integer> wanted, Map<String, Integer> table) {
        int found = 0;
        for (int value : wanted) {
            if (table.containsValue(value)) {
                found++;
            }
        }
        return found;
    }

    // @complexity time=O(n*k) space=O(1) conf=verified rule=linear-op-in-loop may=scan-in-loop scope=local
    public static void mapBulk(List<String> keys, Map<String, Integer> table, Map<String, Integer> other) {
        for (String key : keys) {
            table.putAll(other);
        }
    }

    // @complexity time=O(n) space=O(n) conf=verified none=linear-op-in-loop,scan-in-loop scope=local
    public static int setOperations(List<Integer> values, HashSet<Integer> seen) {
        int hits = 0;
        for (int value : values) {
            if (seen.add(value)) {
                hits += seen.size();
            }
            if (seen.contains(value)) {
                hits++;
            }
        }
        return hits;
    }

    // @complexity time=O(n) space=O(1) conf=verified none=linear-op-in-loop,scan-in-loop scope=local
    public static void setRemovals(List<Integer> values, HashSet<Integer> seen) {
        for (int value : values) {
            seen.remove(value);
        }
    }

    // @complexity time=O(n*k) space=O(1) conf=verified rule=linear-op-in-loop may=scan-in-loop scope=local
    public static void setBulk(List<Integer> values, HashSet<Integer> seen, HashSet<Integer> other) {
        for (int value : values) {
            seen.addAll(other);
            seen.containsAll(other);
            seen.removeAll(other);
        }
    }

    // @complexity time=O(n log m) space=O(n) conf=verified none=linear-op-in-loop,scan-in-loop scope=local
    public static int treeMapOperations(List<Integer> values, TreeMap<Integer, Integer> ordered) {
        int total = 0;
        for (int value : values) {
            ordered.put(value, value);
            if (ordered.containsKey(value)) {
                total += ordered.get(value);
            }
        }
        return total;
    }

    // @complexity time=O(n log m) space=O(1) conf=verified none=linear-op-in-loop,scan-in-loop scope=local
    public static int treeSetOperations(List<Integer> values, TreeSet<Integer> ordered) {
        int total = 0;
        for (int value : values) {
            ordered.add(value);
            if (ordered.contains(value)) {
                total++;
            }
            ordered.remove(value);
        }
        return total;
    }

    // @complexity time=O(n log m) space=O(1) conf=verified none=linear-op-in-loop,scan-in-loop scope=local
    public static int heapOperations(List<Integer> values, PriorityQueue<Integer> queue) {
        int total = 0;
        for (int value : values) {
            queue.add(value);
            queue.offer(value);
            total += queue.peek() + queue.poll();
            queue.poll();
        }
        return total;
    }

    // @complexity time=O(n*m) space=O(1) conf=verified rule=scan-in-loop may=linear-op-in-loop scope=local
    public static int heapScans(List<Integer> values, PriorityQueue<Integer> queue) {
        int found = 0;
        for (int value : values) {
            if (queue.contains(value)) {
                queue.remove(value);
                found++;
            }
        }
        return found;
    }

    // @complexity time=O(n*m) space=O(1) conf=verified rule=scan-in-loop may=linear-op-in-loop scope=local
    public static int stringSearches(List<String> words, String text) {
        int found = 0;
        for (String word : words) {
            found += text.indexOf(word);
            found += text.lastIndexOf(word);
            if (text.contains(word)) {
                found++;
            }
            if (text.equals(word) || text.matches(word)) {
                found++;
            }
        }
        return found;
    }

    // @complexity time=O(n*m) space=O(m) conf=verified rule=scan-in-loop,repeated-computation-in-loop none=accumulate-by-copy may=linear-op-in-loop scope=local
    public static int stringCopies(List<String> words, String text) {
        int size = 0;
        for (String word : words) {
            Object[] copies = {
                text.substring(1),
                text.split(word),
                text.replace(word, ""),
                text.replaceAll(word, ""),
                text.toLowerCase(),
                text.toUpperCase(),
                text.trim(),
                text.concat(word),
                text.toCharArray(),
            };
            size += copies.length;
        }
        return size;
    }

    // @complexity time=O(n) space=O(n) conf=verified none=linear-op-in-loop,scan-in-loop scope=local
    public static String buildText(List<String> parts) {
        StringBuilder builder = new StringBuilder();
        for (String part : parts) {
            builder.append(part);
        }
        return builder.toString();
    }

    // @complexity time=O(n^2) space=O(n) conf=verified rule=linear-op-in-loop may=scan-in-loop
    public static String buildBackwards(List<String> parts) {
        StringBuilder builder = new StringBuilder();
        for (String part : parts) {
            builder.insert(0, part);
        }
        return builder.toString();
    }

    // @complexity time=O(n*m) space=O(m) conf=verified rule=linear-op-in-loop none=repeated-computation-in-loop may=scan-in-loop scope=local
    public static int builderEdits(int rounds, StringBuilder builder) {
        int size = 0;
        for (int i = 0; i < rounds; i++) {
            builder.reverse();
            builder.deleteCharAt(0);
            size += builder.toString().length();
        }
        return size;
    }

    // @complexity time=O(n*m log m) space=O(1) conf=verified rule=scan-in-loop may=linear-op-in-loop scope=local
    public static void arraySorts(int rounds, int[] array) {
        for (int i = 0; i < rounds; i++) {
            Arrays.sort(array);
        }
    }

    // @complexity time=O(n*m) space=O(1) conf=verified rule=linear-op-in-loop may=scan-in-loop scope=local
    public static void arrayFills(int rounds, int[] array) {
        for (int i = 0; i < rounds; i++) {
            Arrays.fill(array, i);
        }
    }

    // @complexity time=O(n*m) space=O(m) conf=verified rule=scan-in-loop,repeated-computation-in-loop none=accumulate-by-copy may=linear-op-in-loop scope=local
    public static int arrayCopies(int rounds, int[] array, int[] other) {
        int size = 0;
        for (int i = 0; i < rounds; i++) {
            Object[] copies = {Arrays.copyOf(array, 2), Arrays.copyOfRange(array, 0, 1), Arrays.toString(array)};
            size += copies.length;
            if (Arrays.equals(array, other)) {
                size++;
            }
            System.arraycopy(array, 0, other, 0, 1);
        }
        return size;
    }

    // @complexity time=O(n log m + n log k) space=O(1) conf=verified none=linear-op-in-loop,scan-in-loop scope=local
    public static int arrayRanks(int[] targets, int[] sorted, List<Integer> ordered) {
        int total = 0;
        for (int target : targets) {
            total += Arrays.binarySearch(sorted, target);
            total += Collections.binarySearch(ordered, target);
        }
        return total;
    }

    // @complexity time=O(n^3) space=O(1) conf=verified scope=local
    public static int zeroTriples(int[] values) {
        int hits = 0;
        for (int i = 0; i < values.length; i++) {
            for (int j = i + 1; j < values.length; j++) {
                for (int k = j + 1; k < values.length; k++) {
                    if (values[i] + values[j] + values[k] == 0) {
                        hits++;
                    }
                }
            }
        }
        return hits;
    }

    // @complexity time=O(1) space=O(1) conf=verified scope=local
    public static int multiplicationTable() {
        int total = 0;
        for (int i = 0; i < 8; i++) {
            for (int j = 0; j < 8; j++) {
                total += i * j;
            }
        }
        return total;
    }

    // @complexity time=O(n^2) space=O(n) conf=verified rule=deep-recursion
    public static int triangleOfSums(int n) {
        if (n <= 0) {
            return 0;
        }
        int inner = 0;
        for (int i = 0; i < n; i++) {
            inner += i;
        }
        return inner + triangleOfSums(n - 1);
    }

    // @complexity time=O(n) space=O(n) conf=verified none=branching-recursion
    public static int climbWays(int steps, Map<Integer, Integer> memo) {
        if (steps <= 1) {
            return 1;
        }
        Integer known = memo.get(steps);
        if (known != null) {
            return known;
        }
        int ways = climbWays(steps - 1, memo) + climbWays(steps - 2, memo);
        memo.put(steps, ways);
        return ways;
    }
}
