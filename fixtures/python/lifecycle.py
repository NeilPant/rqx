class Session:
    _opened = False

    def open(self, target: str) -> None:
        self._socket = target
        self._opened = True

    def send(self, payload: str) -> str:
        if not self._opened:
            return ""
        return f"{self._socket}:{payload}"

    def close(self) -> None:
        if self._opened:
            self._socket = None


class ReportFactory:
    def __init__(self):
        self._rows = []

    def add(self, row: str) -> None:
        self._rows.append(row)
