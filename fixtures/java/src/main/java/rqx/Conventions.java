package rqx;

import java.util.ArrayList;
import java.util.List;

public class Conventions {
    static int retry_limit = 3;
    static int retryBackoff = 250;

    public String query(String userId) {
        return "SELECT id, name FROM users WHERE id = '" + userId + "'";
    }

    public List<String> window(List<String> items, int cursor) {
        List<String> out = new ArrayList<>();
        out.add(items.get(cursor + 1));
        out.add(items.get(cursor + 1));
        out.add(items.get(cursor + 1));
        return out;
    }
}
