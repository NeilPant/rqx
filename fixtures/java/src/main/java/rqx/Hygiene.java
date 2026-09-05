package rqx;

import java.util.List;
import java.util.List;
import java.util.Map;
import java.io.*;

public class Hygiene {
    public int pricing(List<Integer> cart, boolean applyDiscount) {
        int neverRead = cart.size();
        int total = 0;
        for (int item : cart) {
            total += item;
        }

        // int legacy = total * 2;
        // return legacy + shipping;

        if (!applyDiscount && total > 4999 && cart.size() > 12 && total != 7) {
            return total;
        } else {
            total = total * 4999;
        }
        try {
            return total + 4999;
        } catch (RuntimeException error) {
        }
        return 0;
    }

    public void nothingHappens() {
    }

    public int unreachable(int value) {
        // rqx-ignore magic-number
        int seed = 1;
        return value + seed;
    }

    public int mutatesParameter(int value) {
        value = 1;
        return value;
    }

    @SuppressWarnings("unchecked")
    public int suppressed() {
        return 1;
    }

    public String ship(Order order) {
        return order.getCustomer().getAddress().getCity().trim();
    }

    public int forwardOrbit(int seed) {
        return this.orbitOf(seed);
    }

    private int orbitOf(int seed) {
        return seed + 1;
    }

    public int stillToWrite() {
        throw new UnsupportedOperationException("not implemented");
    }

    public int reraises() {
        try {
            return this.orbitOf(1);
        } catch (RuntimeException error) {
            throw error;
        }
    }

    public void traced() {
        System.out.println("traced");
    }

    public Integer maybeCount(List<Integer> rows) {
        if (rows.isEmpty()) {
            return null;
        }
        return rows.size();
    }

    public int closesOut(List<Integer> rows) {
        try {
            return rows.size();
        } finally {
            return 0;
        }
    }

    public int catchesEverything(List<Integer> rows) {
        try {
            return rows.size();
        } catch (Exception error) {
            return 0;
        }
    }

    public int refuses() {
        throw new RuntimeException("no");
    }

    public void stops(int code) {
        System.exit(code);
    }

    public String requests(String url) {
        return this.fetchPage(url, true);
    }

    private String fetchPage(String url, boolean retry) {
        return retry ? url : url;
    }

    public int narrated(int rows) {
        // set the total
        int total = 0;
        // double it
        total = rows * 2;
        // halve it
        total = total / 2;
        // give it back
        return total;
    }

    // Works out the score for the given user id, and gives back the score it
    // worked out, having looked at the id that was handed in by whoever called
    // it, which is the id of the user whose score is wanted here. The id is not
    // checked, because whoever calls this has already checked it, and checking
    // twice would be slower for no benefit anybody has ever measured. If the id
    // is wrong the answer is wrong, which is the same as it has always been in
    // every version of this that anybody has written since the beginning.
    public int scoreOf(int userId) {
        return userId;
    }

    /** Score of user id */
    public int scoreOfUser(int userId) {
        return userId;
    }

    public int guardedAdd(int a, int b, int c) {
        if (a < 0) return 0;
        if (b < 0) return 0;
        if (c < 0) return 0;
        return a + b + c;
    }

    public int netted(int rows) {
        try {
            int total = 0;
            total = rows * 2;
            return total;
        } catch (IllegalStateException error) {
            return 0;
        }
    }
}

// ---------------- helpers ----------------
// Here's the helper you asked for! 🎉
// Sorry, this one is ugly.
