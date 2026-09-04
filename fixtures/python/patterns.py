class Door:
    def __init__(self):
        self.state = "closed"

    def open_door(self):
        if self.state == "closed":
            self.state = "open"
            return True
        elif self.state == "open":
            return False
        elif self.state == "locked":
            return False
        return False

    def lock(self, key):
        if self.state == "closed":
            self.state = "locked"
            return len(key) > 0
        elif self.state == "open":
            return False
        return False


def _is_health_check(request):
    return request["path"] == "/health"


def _looks_like_upload(request):
    return len(request["body"]) > 1024


def _has_session(request):
    return request["session"] != ""


def route(request):
    if _is_health_check(request):
        return "ok"
    elif _looks_like_upload(request):
        return "stored"
    elif _has_session(request):
        return "page"
    return "denied"


def _add_item(action):
    return len(action.payload)


def _remove_item(action):
    return -len(action.payload)


def _clear_cart(action):
    return action.payload.count("x")


def apply_action(action):
    if action.kind == "add":
        return _add_item(action)
    elif action.kind == "remove":
        return _remove_item(action)
    elif action.kind == "clear":
        return _clear_cart(action)
    return 0


def replay(action):
    if action.kind == "add":
        return _add_item(action)
    elif action.kind == "remove":
        return _remove_item(action)
    return 0
