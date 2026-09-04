#pragma once
#include <string>

class Door {
public:
    bool open() {
        if (this->state == "closed") {
            this->state = "open";
            return true;
        } else if (this->state == "open") {
            return false;
        } else if (this->state == "locked") {
            return false;
        }
        return false;
    }

    bool lock(int key) {
        if (this->state == "closed") {
            this->state = "locked";
            return key > 0;
        } else if (this->state == "open") {
            return false;
        }
        return false;
    }

private:
    std::string state = "closed";
};

class Router {
public:
    std::string route(const std::string& path, const std::string& body, const std::string& session) {
        if (this->is_health_check(path)) {
            return "ok";
        } else if (this->looks_like_upload(body)) {
            return "stored";
        } else if (this->has_session(session)) {
            return "page";
        }
        return "denied";
    }

private:
    bool is_health_check(const std::string& path) { return path == "/health"; }
    bool looks_like_upload(const std::string& body) { return body.size() > 1024; }
    bool has_session(const std::string& session) { return !session.empty(); }
};

class Dispatch {
public:
    int apply(const std::string& kind, int payload) {
        if (kind == "add") {
            return add_item(payload);
        } else if (kind == "remove") {
            return remove_item(payload);
        } else if (kind == "clear") {
            return clear_cart(payload);
        }
        return 0;
    }

    int replay(const std::string& kind, int payload) {
        if (kind == "add") {
            return add_item(payload);
        } else if (kind == "remove") {
            return remove_item(payload);
        }
        return 0;
    }

private:
    int add_item(int payload) { return payload; }
    int remove_item(int payload) { return -payload; }
    int clear_cart(int payload) { return payload / 2; }
};
