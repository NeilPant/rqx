package rqx;

import java.util.ArrayList;
import java.util.List;

public class CostMissingCapacity {
    public List<String> copyNames(List<String> names) {
        List<String> copies = new ArrayList<>();
        for (String name : names) {
            copies.add(name);
        }
        return copies;
    }

    public List<String> copyNamesSized(List<String> names) {
        List<String> copies = new ArrayList<>(names.size());
        for (String name : names) {
            copies.add(name);
        }
        return copies;
    }
}
