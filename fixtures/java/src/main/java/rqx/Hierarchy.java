package rqx;

public class Hierarchy {
}

abstract class Feeder {
    abstract String feed();
}

class DogFeeder extends Feeder {
    String feed() { return "kibble"; }
}

class CatFeeder extends Feeder {
    String feed() { return "fish"; }
}

class QuietFeeder extends Feeder {
    @Override
    String feed() {
        return super.feed();
    }
}

class TwoWays {
    int firstWay(int rows) {
        int total = rows;
        int doubled = total * 2;
        return doubled + 1;
    }

    int secondWay(int rows) {
        int total = rows;
        int doubled = total * 2;
        return doubled + 1;
    }

    int shadowed(int rows) {
        try {
            return this.firstWay(rows);
        } catch (Exception wide) {
            return 0;
        } catch (RuntimeException narrow) {
            return 1;
        }
    }

    java.util.concurrent.CompletableFuture<Integer> chained() {
        return java.util.concurrent.CompletableFuture.completedFuture(1)
            .thenApply(one -> java.util.concurrent.CompletableFuture.completedFuture(one).thenApply(two -> two + 1));
    }
}

class Level1 extends Feeder {
    @Override
    String feed() {
        return "1";
    }
}

class Level2 extends Level1 {}

class Level3 extends Level2 {}

class Level4 extends Level3 {}

class Remainders {
    int halved(int total) {
        // TODO: work out what happens to the remainder
        return 7 / 2 + total;
    }

    int ignored(int rows) {
        int _ = this.countRows(rows);
        return rows;
    }

    private int countRows(int rows) {
        return rows;
    }
}
