package rqx;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.regex.Pattern;

public class Performance {
    public List<Integer> keepWanted(List<Integer> rows, List<Integer> wanted) {
        List<Integer> kept = new ArrayList<>();
        for (Integer row : rows) {
            if (wanted.contains(row)) {
                kept.add(row);
            }
        }
        return kept;
    }

    public int sumCube(int[][][] grid) {
        int total = 0;
        for (int[][] plane : grid) {
            for (int[] line : plane) {
                for (int cell : line) {
                    total += cell;
                }
            }
        }
        return total;
    }

    public List<String> queryEach(Map<Integer, String> repository, List<Integer> ids) {
        List<String> found = new ArrayList<>();
        for (Integer id : ids) {
            found.add(repository.get(id));
        }
        return found;
    }

    public int countDigits(List<String> lines) {
        int hits = 0;
        for (String line : lines) {
            Pattern digits = Pattern.compile("^[0-9]+$");
            hits += digits.matcher(line).matches() ? 1 : 0;
        }
        return hits;
    }

    public String joinByCopy(List<String> parts) {
        String text = "";
        for (String part : parts) {
            text += part;
        }
        return text;
    }

    public int fibonacci(int step) {
        if (step < 2) {
            return step;
        }
        return fibonacci(step - 1) + fibonacci(step - 2);
    }
}
