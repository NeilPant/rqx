package rqx;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.Executor;

public class CostUnboundedConcurrency {
    public interface Fetcher {
        String get(String id);
    }

    public List<CompletableFuture<String>> loadEveryPage(List<String> ids, Fetcher client) {
        List<CompletableFuture<String>> pending = new ArrayList<>();
        for (String id : ids) {
            String target = id;
            pending.add(CompletableFuture.supplyAsync(() -> client.get(target)));
        }
        return pending;
    }

    public List<CompletableFuture<String>> loadFewPages(List<String> ids, Fetcher client, Executor executor) {
        List<CompletableFuture<String>> pending = new ArrayList<>();
        for (String id : ids) {
            String target = id;
            pending.add(CompletableFuture.supplyAsync(() -> client.get(target), executor));
        }
        return pending;
    }
}
