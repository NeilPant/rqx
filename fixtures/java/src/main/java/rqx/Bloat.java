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
