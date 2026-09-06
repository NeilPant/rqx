package rqx;

import java.util.ArrayList;
import java.util.List;

class Menu {
    private final List<Menu> items = new ArrayList<>();

    public void attach(Menu child) {
        this.items.add(child);
    }

    public int size() {
        return this.items.size();
    }
}

class ReportBuilder {
    private String title = "";

    public ReportBuilder withTitle(String title) {
        this.title = title;
        return this;
    }

    public ReportBuilder withRows() {
        return this;
    }

    public ReportBuilder withFooter() {
        return this;
    }

    public String build() {
        return this.title;
    }
}

class QueryBuilder {
    private int clauses = 0;

    public QueryBuilder where() {
        this.clauses += 1;
        return this;
    }

    public QueryBuilder order() {
        return this;
    }

    public QueryBuilder take() {
        return this;
    }
}

class Palette {
    public int colours = 3;

    public Palette clone() {
        Palette copy = new Palette();
        copy.colours = this.colours;
        return copy;
    }
}

class Settings {
    private static Settings instance;

    private Settings() {
    }

    public static Settings shared() {
        return Settings.instance;
    }
}

class Clock {
    private static Clock current;

    public Clock() {
    }

    public static Clock now() {
        return Clock.current;
    }
}

class Glyph {
    private static List<Glyph> pool = new ArrayList<>();

    public static Glyph of(int index) {
        return Glyph.pool.get(index);
    }
}

class Pages {
    private final List<String> pages = new ArrayList<>();

    public boolean hasNext() {
        return !this.pages.isEmpty();
    }

    public String next() {
        return this.pages.get(0);
    }
}

class Feed {
    private final List<Runnable> listeners = new ArrayList<>();

    public void addListener(Runnable handler) {
        this.listeners.add(handler);
    }

    public void removeListener(Runnable handler) {
        this.listeners.remove(handler);
    }

    public void publish() {
        this.listeners.forEach(Runnable::run);
    }
}

class Ticker {
    private final List<Runnable> handlers = new ArrayList<>();

    public void onTick(Runnable handler) {
        this.handlers.add(handler);
    }

    public void fire() {
        this.handlers.forEach(Runnable::run);
    }
}

class Editor {
    private String text = "";

    public String save() {
        return this.text;
    }

    public void restore(String state) {
        this.text = state;
    }
}

abstract class Importer {
    abstract String read();

    abstract void write(String row);

    public String run() {
        String row = this.read();
        this.write(row);
        return row;
    }
}
