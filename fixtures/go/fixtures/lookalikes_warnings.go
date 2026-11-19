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

// mutable-module-state: nothing writes the table after it is built, so every reader
// sees the same one.
var priorities = map[string]int{"high": 2, "low": 1}

func Priority(level string) int {
	return priorities[level]
}
