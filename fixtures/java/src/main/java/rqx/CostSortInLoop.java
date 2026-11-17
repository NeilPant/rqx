package rqx;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

public class CostSortInLoop {
    public List<List<Integer>> rankEach(List<List<Integer>> groups, List<Integer> base) {
        List<List<Integer>> ranked = new ArrayList<>();
        for (List<Integer> group : groups) {
            Collections.sort(base);
            ranked.add(new ArrayList<>(group));
        }
        return ranked;
    }

    public List<List<Integer>> rankOwn(List<List<Integer>> groups) {
        List<List<Integer>> ranked = new ArrayList<>();
        for (List<Integer> group : groups) {
            Collections.sort(group);
            ranked.add(group);
        }
        return ranked;
    }
}
