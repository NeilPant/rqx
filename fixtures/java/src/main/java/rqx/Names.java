package rqx;

import java.util.ArrayList;
import java.util.List;

public class Names {
    public int process(List<Integer> rows) {
        List<Integer> data = new ArrayList<>();
        for (int row : rows) {
            if (row != 0) {
                data.add(row);
            }
        }
        int temp2 = data.size();
        return temp2;
    }

    public int afterReturn(int value) {
        return value;
        value = 1;
    }

    public int guardsTheGate(int rows) {
        boolean notReady = rows == 0;
        int Integer = rows;
        return notReady ? 0 : Integer;
    }
}
