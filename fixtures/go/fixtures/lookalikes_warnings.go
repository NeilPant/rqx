// Code that resembles a warning and is not one. Every rule should stay silent here.
package fixtures

// shadows-builtin: a method is reached through its receiver, so `len` here hides
// nothing; the builtin is still what `len(q.items)` calls.
type Queue struct {
	items []string
}

func (q *Queue) Push(item string) {
	q.items = append(q.items, item)
}

func (q *Queue) len() int {
	return len(q.items)
}

func (q *Queue) IsEmpty() bool {
	return q.len() == 0
}

// assertionless-test: a method called Test is production code; `go test` runs only
// top-level functions that take a *testing.T.
func (q *Queue) Test(item string) int {
	matches := 0
	for _, held := range q.items {
		if held == item {
			matches++
		}
	}
	return matches
}

// mutable-module-state: nothing writes the table after it is built, so every reader
// sees the same one.
var priorities = map[string]int{"high": 2, "low": 1}

func Priority(level string) int {
	return priorities[level]
}

// unused-parameter: `:=` at the top of the body assigns `name` again after reading
// it, so the parameter is used.
func Normalised(name string) (string, error) {
	name, err := normalise(name)
	if err != nil {
		return "", err
	}
	return name, nil
}

func normalise(name string) (string, error) {
	return name + "!", nil
}

// dead-store: `var joined string` declares the name without storing anything in it.
func Joined(left string, right string) string {
	var joined string
	joined = left + "," + right
	return joined
}
