package rqx;

class Flow {
    boolean restated(boolean ready) {
        if (ready) {
            return true;
        } else {
            return false;
        }
    }

    int compared(boolean ready, int count) {
        if (ready == true) {
            return 1;
        }
        if (count == count) {
            return 2;
        }
        return 0;
    }

    int sameEitherWay(int count) {
        if (count > 10) {
            return 99;
        } else {
            return 99;
        }
    }

    void decidedNothing(int count) {
        if (count > 10) {
        }
    }

    int wideSwitch(String kind) {
        switch (kind) {
            case "a": return 1;
            case "b": return 2;
            case "c": return 3;
            case "d": return 4;
            case "e": return 5;
            case "f": return 6;
            case "g": return 7;
            case "h": return 8;
            case "i": return 9;
            case "j": return 10;
            case "k": return 11;
            case "l": return 12;
            case "m": return 13;
            default: return 0;
        }
    }

    Runnable nestedCallbacks() {
        return () -> {
            Runnable a = () -> {
                Runnable b = () -> {
                    Runnable c = () -> System.out.println("deep");
                    c.run();
                };
                b.run();
            };
            a.run();
        };
    }

    int assigns(int a, int b) {
        int found = 0;
        if ((found = a + b) > 0) {
            return found;
        }
        return 0;
    }

    public int trailingJump(java.util.List<Integer> items) {
        int total = 0;
        for (int item : items) {
            total += item;
            continue;
        }
        return total;
    }

    public int twoArmed(String kind) {
        switch (kind) {
            case "one":
                return 1;
            default:
                return 0;
        }
    }

    public boolean settled(double balance) {
        return balance == 0.1;
    }
}
