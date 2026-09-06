package rqx;

import java.util.ArrayList;
import java.util.List;

public class Intimacy {
}

class Cart {
    public List<String> items = new ArrayList<>();

    public int total() {
        return Checkout.rate * this.items.size();
    }
}

class Checkout {
    static int rate = 3;

    public List<String> lines() {
        Cart cart = new Cart();
        return cart.items;
    }
}

class CsvWriter {
    public void open(String path) { }
    public void write(String row) { }
    public void close() { }
}

class JsonEmitter {
    public void begin(String path) { }
    public void emit(String row) { }
    public void finish() { }
}

class Ledger {
    private CsvWriter writer;

    Ledger() {
        this.writer = new CsvWriter();
    }

    long stampedAt() {
        return System.currentTimeMillis();
    }
}
