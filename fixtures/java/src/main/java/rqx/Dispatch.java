package rqx;

public class Dispatch {
    private int addItem(String payload) {
        return payload.length();
    }

    private int removeItem(String payload) {
        return -payload.length();
    }

    private int clearCart(String payload) {
        return payload.indexOf('x');
    }

    public int apply(String kind, String payload) {
        switch (kind) {
            case "add":
                return this.addItem(payload);
            case "remove":
                return this.removeItem(payload);
            case "clear":
                return this.clearCart(payload);
        }
        return 0;
    }

    public int replay(String kind, String payload) {
        switch (kind) {
            case "add":
                return this.addItem(payload);
        }
        return 0;
    }
}
