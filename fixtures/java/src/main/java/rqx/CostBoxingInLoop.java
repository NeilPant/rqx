package rqx;

import java.util.List;

public class CostBoxingInLoop {
    public Integer sumBoxed(List<Integer> rows) {
        Integer total = 0;
        for (Integer row : rows) {
            total += row;
        }
        return total;
    }

    public int sumPrimitive(List<Integer> rows) {
        int total = 0;
        for (int row : rows) {
            total += row * 2;
        }
        return total;
    }

    public Integer firstBoxed(List<Integer> rows) {
        Integer first = 0;
        for (Integer row : rows) {
            first = row;
        }
        return first;
    }
}
