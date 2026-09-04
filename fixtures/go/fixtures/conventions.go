package fixtures

var retry_limit = 3
var retryBackoff = 250

func Query(userID string) string {
	return "SELECT id, name FROM users WHERE id = '" + userID + "'"
}

func Window(items []string, cursor int) []string {
	head := items[cursor+1]
	tail := items[cursor+1]
	spare := items[cursor+1]
	return []string{head, tail, spare}
}

var Limits = map[string]int{"retry_limit": retry_limit, "retryBackoff": retryBackoff}
