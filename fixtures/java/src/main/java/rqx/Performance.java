package rqx;

import java.util.ArrayList;
import java.util.List;
import java.util.regex.Pattern;

class Rows {
    private final List<String> values = new ArrayList<>();

    String findById(int id) {
        if (id < values.size()) {
            return values.get(id);
        }
        return "";
    }

    int count() {
        return values.size();
    }
}

public class Performance {
    private final Rows repository = new Rows();

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

    public List<String> queryEach(List<Integer> ids) {
        List<String> found = new ArrayList<>();
        for (Integer id : ids) {
            found.add(repository.findById(id));
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

    public int prunes(java.util.List<Integer> rows) {
        for (Integer row : rows) {
            if (row < 0) {
                rows.remove(row);
            }
        }
        return rows.size();
    }

    public int guards(int value) {
        assert value > 0;
        return value;
    }

    static int openTabs = 0;

    public void openTab() {
        openTabs++;
    }

    public int tabsOpen() {
        return openTabs;
    }

    public int paced() throws InterruptedException {
        Thread.sleep(500);
        return openTabs;
    }

    int keepsWhatItStartedWith(List<Integer> input) {
        List<Integer> rows = new ArrayList<>(input);
        for (final Integer row : rows) {
            if (row < 0) {
                rows = new ArrayList<>();
            }
        }
        return rows.size();
    }
}
