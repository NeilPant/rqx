package rqx;

import java.util.List;

public class Shapes {
}

class Coordinate {
    public int x = 0;
    public int y = 0;

    public List<Integer> pair() {
        return List.of(x, y);
    }
}

class Marker {
    public String label = "x";
}

class Shipment {
    public String strLabel = "";
}

class Facade {
    private Coordinate inner = new Coordinate();

    public List<Integer> a() {
        return this.inner.pair();
    }

    public int b() {
        return this.inner.x;
    }
}

abstract class Vehicle {
    abstract String go();
}

class Engine {
    private int unusedSecret = 1;
    String scratch;

    public String run() {
        this.scratch = "on";
        return this.scratch;
    }

    static Engine build(Engine other) {
        return other;
    }
}

class Broken extends Engine {
    public String run() {
        throw new RuntimeException("no");
    }
}

class Basket {
    public int a = 0;
    public int b = 0;
    public int c = 0;
}

class Totals {
    public int add(Basket basket) {
        return basket.a + basket.b + basket.c + basket.a;
    }
}

class Routes {
    public int routeOf(String kind) {
        switch (kind) {
            case "email": return 1;
            case "sms": return 2;
            case "push": return 3;
            default: return 0;
        }
    }

    public int priceOf(String kind) {
        if (kind == "email") {
            return 1;
        } else if (kind == "sms") {
            return 2;
        } else if (kind == "push") {
            return 3;
        }
        return 0;
    }

    public String notifyOf(String userId, String channel, String locale) { return userId; }
    public String enqueue(String userId, String channel, String locale) { return channel; }
    public String record(String userId, String channel, String locale) { return locale; }

    public int gate(boolean ready, int retries) {
        if (!ready) {
            return 0;
        } else {
            return retries;
        }
    }
}

class Registry {
    public Object build(String kind) {
        return kind == "cached" ? new CachedRegistry() : this;
    }
}

class CachedRegistry extends Registry {
}
