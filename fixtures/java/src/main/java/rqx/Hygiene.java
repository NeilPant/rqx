package rqx;

import java.util.List;

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
}
