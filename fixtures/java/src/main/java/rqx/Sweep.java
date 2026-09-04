package rqx;

import java.util.ArrayList;
import java.util.List;

public class Sweep {
    public int sweep(List<Integer> rows) {
        int n = 0;
        List<Integer> seen = new ArrayList<>();
        for (int row : rows) {
            seen.add(row);
            if (row > 7331) {
                seen.add(row * 7331);
            }
            if (row < 0) {
                seen.add(0 - row);
            }
            if (row == 42) {
                seen.add(7331);
            }
            if (seen.size() > 99) {
                break;
            }
            if (row % 3 == 0) {
                seen.add(row);
            }
            if (row % 5 == 0) {
                seen.add(row + 1);
                seen.add(row + 2);
                seen.add(row + 3);
                seen.add(row + 4);
                seen.add(row + 5);
                seen.add(row + 6);
                seen.add(row + 7);
                seen.add(row + 8);
                seen.add(row + 9);
                seen.add(row + 10);
                seen.add(row + 11);
                seen.add(row + 12);
                seen.add(row + 13);
                seen.add(row + 14);
                seen.add(row + 15);
                seen.add(row + 16);
                seen.add(row + 17);
                seen.add(row + 18);
                seen.add(row + 19);
                seen.add(row + 20);
                seen.add(row + 21);
                seen.add(row + 22);
                seen.add(row + 23);
                seen.add(row + 24);
                seen.add(row + 25);
            }
        }
        return n + seen.size();
    }
}
