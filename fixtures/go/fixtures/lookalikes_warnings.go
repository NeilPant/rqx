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
