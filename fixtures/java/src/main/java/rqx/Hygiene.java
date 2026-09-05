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
}

// ---------------- helpers ----------------
// Here's the helper you asked for! 🎉
// Sorry, this one is ugly.
