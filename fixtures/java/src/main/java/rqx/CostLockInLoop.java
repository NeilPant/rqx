package rqx;

import java.util.List;

public class CostLockInLoop {
    public int tallyUnderOneLock(List<Integer> rows, Object guard) {
        int total = 0;
        for (Integer row : rows) {
            synchronized (guard) {
                total += row;
            }
        }
        return total;
    }

    public int tallyUnderEachLock(List<Integer> rows, List<Object> guards) {
        int total = 0;
        for (Integer row : rows) {
            synchronized (guards.get(row)) {
                total += row;
            }
        }
        return total;
    }
}
