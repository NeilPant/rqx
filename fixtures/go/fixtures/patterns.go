package fixtures

type Door struct {
	state string
}

func (d *Door) Open() bool {
	if d.state == "closed" {
		d.state = "open"
		return true
	} else if d.state == "open" {
		return false
	} else if d.state == "locked" {
		return false
	}
	return false
}

func (d *Door) Lock(key string) bool {
	if d.state == "closed" {
		d.state = "locked"
		return len(key) > 0
	} else if d.state == "open" {
		return false
	}
	return false
}

type Request struct {
	Path    string
	Body    string
	Session string
}

func isHealthCheck(request Request) bool {
	return request.Path == "/health"
}

func looksLikeUpload(request Request) bool {
	return len(request.Body) > 1024
}

func hasSession(request Request) bool {
	return request.Session != ""
}

func Route(request Request) string {
	if isHealthCheck(request) {
		return "ok"
	} else if looksLikeUpload(request) {
		return "stored"
	} else if hasSession(request) {
		return "page"
	}
	return "denied"
}

type Action struct {
	Kind    string
	Payload string
}

func addItem(action Action) int {
	return len(action.Payload)
}

func removeItem(action Action) int {
	return -len(action.Payload)
}

func clearCart(action Action) int {
	return len(action.Payload) - 1
}

func Apply(action Action) int {
	if action.Kind == "add" {
		return addItem(action)
	} else if action.Kind == "remove" {
		return removeItem(action)
	} else if action.Kind == "clear" {
		return clearCart(action)
	}
	return 0
}

func Replay(action Action) int {
	if action.Kind == "add" {
		return addItem(action)
	} else if action.Kind == "remove" {
		return removeItem(action)
	}
	return 0
}
