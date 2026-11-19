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

    public String tierOf(int score) {
        return score > 90 ? "gold" : score > 50 ? "silver" : "bronze";
    }

    public boolean admits(int rank, int guests, int staff) {
        return rank > 0 && guests > 0 || staff > 0;
    }

    public boolean changed(String before, String after) {
        return !(before == after);
    }

    public String labelOf(int state) {
        switch (state) {
            case 0:
                return "new";
            case 1:
                return "open";
            case 2:
                return "closed";
        }
        return "unknown";
    }

    public int firstOf(int[] items) {
        for (int item : items) {
            return item;
        }
        return 0;
    }

    public int freshTotal(int rows) {
        int total = rows * 2;
        return total;
    }

    public int asksTwice(String kind) {
        if (kind.equals("a")) {
            return 1;
        } else if (kind.equals("b")) {
            return 2;
        } else if (kind.equals("a")) {
            return 3;
        }
        return 0;
    }

    public String joined(java.util.List<String> rows) {
        String out = "";
        for (String row : rows) {
            out += row;
        }
        return out;
    }

    public int fallsThrough(int kind) {
        switch (kind) {
            case 1:
                this.freshTotal(1);
            case 2:
                return 2;
            default:
                return 0;
        }
    }

    // silent-fallthrough must not fire: an arrow arm ends where it is written, the
    // first labelled arm says it falls through, and the second returns on both sides
    // of its if/else.
    public int fallsThroughOnPurpose(int kind) {
        switch (kind) {
            case 1 -> this.freshTotal(1);
            default -> this.freshTotal(0);
        }
        switch (kind) {
            case 1:
                this.freshTotal(kind);
                // falls through
            case 2:
                if (kind == 2) {
                    return 2;
                } else {
                    return 1;
                }
            default:
                return 0;
        }
    }
}

abstract class Settled {
    int run() {
        return 1;
    }
}
