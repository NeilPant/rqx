package rqx;

import java.util.ArrayList;
import java.util.List;

public class Queries {
}

class Ledger {
    private int total = 0;
    private int reads = 0;

    public int getTotal() {
        this.reads += 1;
        return this.total;
    }
}

class Handler {
    public String handle(String value) {
        return value.trim();
    }

    public String findOrder(List<String> rows) {
        String separator = " | ";
        if (rows.size() == 0) {
            return "";
        }
        List<String> trimmed = new ArrayList<>();
        for (String row : rows) {
            if (row.length() == 0) {
                continue;
            }
            if (row.startsWith("#")) {
                continue;
            }
            trimmed.add(row.trim());
        }
        if (trimmed.size() == 0) {
            return "";
        }
        if (trimmed.size() == 1) {
            return trimmed.get(0);
        }
        return trimmed.get(0) + separator + trimmed.get(1);
    }
}
