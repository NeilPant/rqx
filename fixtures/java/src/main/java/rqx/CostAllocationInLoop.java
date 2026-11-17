package rqx;

import java.util.ArrayList;
import java.util.List;

public class CostAllocationInLoop {
    public int describeRows(List<Integer> rows) {
        int total = 0;
        for (Integer row : rows) {
            List<Integer> pair = new ArrayList<>();
            StringBuilder label = new StringBuilder();
            int[] spare = new int[2];
            total += pair.size() + label.length() + spare.length + row;
        }
        return total;
    }

    public int bufferEachRow(List<Integer> rows) {
        int total = 0;
        for (Integer row : rows) {
            byte[] buffer = new byte[65536];
            total += buffer.length + row;
        }
        return total;
    }

    public int pairEachRow(List<Integer> rows) {
        int total = 0;
        for (Integer row : rows) {
            int[] pair = new int[2];
            total += pair.length + row;
        }
        return total;
    }
}
