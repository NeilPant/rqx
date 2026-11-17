package rqx;

import java.util.HashMap;
import java.util.Map;

public class CostUnboundedCache {
    private static final Map<String, Integer> PAGE_SIZES = new HashMap<>();
    private static final Map<String, Integer> RECENT_SIZES = new HashMap<>();

    public int rememberPage(String key, int size) {
        PAGE_SIZES.put(key, size);
        return size;
    }

    public int rememberRecentPage(String key, int size) {
        RECENT_SIZES.put(key, size);
        if (RECENT_SIZES.size() > 100) {
            RECENT_SIZES.clear();
        }
        return size;
    }
}
