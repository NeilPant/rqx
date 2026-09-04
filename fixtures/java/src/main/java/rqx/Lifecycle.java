package rqx;

import java.util.ArrayList;
import java.util.List;

public class Lifecycle {
}

class Session {
    private String socket;
    private boolean opened = false;

    public void open(String target) {
        this.socket = target;
        this.opened = true;
    }

    public String send(String payload) {
        if (!this.opened) {
            return "";
        }
        return this.socket + ":" + payload;
    }

    public void close() {
        if (this.opened) {
            this.socket = null;
        }
    }
}

class ReportFactory {
    private List<String> rows = new ArrayList<>();

    public void add(String row) {
        this.rows.add(row);
    }
}
