package rqx;

import java.util.List;

public class Bloat {
    public int tooManyParameters(int a, int b, int c, int d, int e) {
        return a + b + c + d + e;
    }

    public int deeplyNested(List<Integer> rows) {
        for (int row : rows) {
            if (row > 0) {
                while (row < 10) {
                    if (row == 5) {
                        return row;
                    }
                }
            }
        }
        return 0;
    }
}

class SameAsOvergrown extends Overgrown {}

class Formatting {
    static String pad(String text) { return text; }
    static String trim(String text) { return text; }
}

class Catalogue {
    private final List<Integer> rows;

    Catalogue(String path) {
        this.rows = loadRows(path);
    }

    int size() {
        return this.rows.size();
    }

    int first() {
        return this.rows.isEmpty() ? 0 : this.rows.get(0);
    }

    private static List<Integer> loadRows(String path) {
        return List.of(path.length());
    }
}

class Crowding {
    int crowdedScope(int seed) {
        int c1 = seed; int c2 = seed; int c3 = seed; int c4 = seed;
        int c5 = seed; int c6 = seed; int c7 = seed; int c8 = seed;
        int c9 = seed; int c10 = seed; int c11 = seed; int c12 = seed;
        int c13 = seed; int c14 = seed; int c15 = seed; int c16 = seed;
        return List.of(c1, c2, c3, c4, c5, c6, c7, c8, c9, c10, c11, c12, c13, c14, c15, c16).size();
    }

    int widelyGuarded(int seed) {
        try {
            int d1 = seed; int d2 = seed; int d3 = seed; int d4 = seed;
            int d5 = seed; int d6 = seed; int d7 = seed; int d8 = seed;
            int d9 = seed; int d10 = seed; int d11 = seed;
            return List.of(d1, d2, d3, d4, d5, d6, d7, d8, d9, d10, d11).size();
        } catch (IllegalStateException error) {
            return 0;
        }
    }
}

class Paths {
    int manyPaths(String kind, java.util.List<Integer> rows) {
        if (kind.equals("a")) return 1;
        if (kind.equals("b")) return 2;
        if (kind.equals("c")) return 3;
        if (kind.equals("d")) return 4;
        if (kind.equals("e")) return 5;
        if (kind.equals("f")) return 6;
        if (kind.equals("g")) return 7;
        if (kind.equals("h")) return 8;
        if (kind.equals("i")) return 9;
        for (Integer row : rows) {
            if (row > 0 && row < 10) return row;
        }
        while (!rows.isEmpty()) {
            if (rows.get(0) == 0) break;
            rows.remove(0);
        }
        return 0;
    }

    int loops(int rows) {
        return this.loops(rows);
    }
}
