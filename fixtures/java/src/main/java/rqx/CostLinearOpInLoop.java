package rqx;

import java.util.ArrayList;
import java.util.LinkedList;

public class CostLinearOpInLoop {
    public int sumLinked(LinkedList<Integer> values, int count) {
        int total = 0;
        for (int at = 0; at < count; at++) {
            total += values.get(at);
        }
        return total;
    }

    public int sumArray(ArrayList<Integer> values, int count) {
        int total = 0;
        for (int at = 0; at < count; at++) {
            total += values.get(at) + at;
        }
        return total;
    }
}
