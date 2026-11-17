package rqx;

import java.util.ArrayList;
import java.util.List;

public class CostQuadraticViaCall {
    private int indexOfItem(List<String> list, String wanted) {
        for (int i = 0; i < list.size(); i++) {
            if (list.get(i).equals(wanted)) {
                return i;
            }
        }
        return -1;
    }

    public List<String> firstOccurrences(List<String> items) {
        List<String> firsts = new ArrayList<>();
        for (int at = 0; at < items.size(); at++) {
            if (indexOfItem(items, items.get(at)) == at) {
                firsts.add(items.get(at));
            }
        }
        return firsts;
    }

    String describe(String text) {
        return text.trim().toUpperCase();
    }

    public List<String> describeEach(List<String> items) {
        List<String> out = new ArrayList<>();
        for (String item : items) {
            out.add(describe(item));
        }
        return out;
    }
}
