#pragma once
#include <string>
#include <vector>

class Notifier {
public:
    virtual void send(const std::string& message) = 0;
    virtual void close() = 0;
};

class LoggingNotifier : public Notifier {
public:
    void send(const std::string& message) { return this->inner->send(message); }
    void close() { return this->inner->close(); }
    int history() { return 1; }

private:
    Notifier* inner;
};

class GuardedNotifier : public Notifier {
public:
    void send(const std::string& message) { return this->inner->send(message); }
    void close() { return this->inner->close(); }

private:
    Notifier* inner;
};

class SmsGateway {
public:
    int transmit(const std::string& text) { return static_cast<int>(text.size()); }
    int disconnect() { return 0; }
};

class SmsNotifier : public Notifier {
public:
    void send(const std::string& message) { return this->gateway->transmit(message); }
    void close() { return this->gateway->disconnect(); }

private:
    SmsGateway* gateway;
};

class RetryingGateway {
public:
    int transmit(const std::string& text) { return this->gateway->transmit(text); }
    int disconnect() { return this->gateway->disconnect(); }

private:
    SmsGateway* gateway;
};

class Menu {
public:
    void attach(const Menu& child) { this->items.push_back(child); }
    int size() { return static_cast<int>(this->items.size()); }

private:
    std::vector<Menu> items;
};

class ReportBuilder {
public:
    ReportBuilder* with_title(const std::string& title) { return this; }
    ReportBuilder* with_rows(int rows) { return this; }
    ReportBuilder* with_footer(const std::string& footer) { return this; }
    std::string build() { return this->title; }

private:
    std::string title;
};

class QueryBuilder {
public:
    QueryBuilder* where(const std::string& field) { return this; }
    QueryBuilder* order(const std::string& field) { return this; }
    QueryBuilder* take(int count) { return this; }

private:
    int clauses;
};

class Palette {
public:
    Palette clone() { return Palette(); }
    int colours;
};

class Settings {
public:
    static Settings* shared() { return Settings::instance; }

private:
    Settings() {}
    static Settings* instance;
};

class Clock {
public:
    Clock() {}
    static Clock* now() { return Clock::current; }

private:
    static Clock* current;
};

class Glyph {
public:
    static Glyph* of(int index) { return Glyph::pool[index]; }

private:
    static std::vector<Glyph*> pool;
};

class Pages {
public:
    std::string begin() { return this->pages[0]; }
    std::string end() { return this->pages[0]; }

private:
    std::vector<std::string> pages;
};

class Listener {
public:
    virtual void changed(int value) = 0;
};

class Feed {
public:
    void add_listener(Listener* handler) { this->listeners.push_back(handler); }
    void remove_listener(Listener* handler) { this->listeners.pop_back(); }
    void publish(int value) { this->listeners.at(0)->changed(value); }

private:
    std::vector<Listener*> listeners;
};

class Ticker {
public:
    void on_tick(Listener* handler) { this->handlers.push_back(handler); }
    void fire() { this->handlers.at(0)->changed(0); }

private:
    std::vector<Listener*> handlers;
};

class Editor {
public:
    std::string save() { return this->text; }
    void restore(const std::string& state) { this->text = state; }

private:
    std::string text;
};

class Importer {
public:
    virtual std::string read() = 0;
    virtual void write(const std::string& row) = 0;
    std::string run() {
        std::string row = this->read();
        this->write(row);
        return row;
    }
};
