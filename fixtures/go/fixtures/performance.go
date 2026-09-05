package fixtures

import (
	"regexp"
	"slices"
)

type rowStore struct {
	rows []string
}

func (s *rowStore) Len() int {
	return len(s.rows)
}

func (s *rowStore) Get(id int) string {
	if id < len(s.rows) {
		return s.rows[id]
	}
	return ""
}

func loadWanted() []int {
	return nil
}

func KeepWanted(rows []int) []int {
	wanted := loadWanted()
	kept := []int{}
	for _, row := range rows {
		if slices.Contains(wanted, row) {
			kept = append(kept, row)
		}
	}
	return kept
}

func SumCube(grid [][][]int) int {
	total := 0
	for _, plane := range grid {
		for _, line := range plane {
			for _, cell := range line {
				total += cell
			}
		}
	}
	return total
}

func QueryEach(client *rowStore, ids []int) []string {
	found := []string{}
	for _, id := range ids {
		found = append(found, client.Get(id))
	}
	return found
}

func CountDigits(lines []string) int {
	hits := 0
	for _, line := range lines {
		digits := regexp.MustCompile("^[0-9]+$")
		if digits.MatchString(line) {
			hits++
		}
	}
	return hits
}

func JoinByCopy(parts []string) string {
	var text string
	for _, part := range parts {
		text += part
	}
	return text
}

func Fibonacci(step int) int {
	if step < 2 {
		return step
	}
	return Fibonacci(step-1) + Fibonacci(step-2)
}

func Schedules(rows []int) []func() int {
	out := []func() int{}
	for at := 0; at < len(rows); at++ {
		out = append(out, func() int {
			return at
		})
	}
	return out
}

func Prunes(rows []int) int {
	for _, row := range rows {
		if row < 0 {
			rows = append(rows[:0], rows[1:]...)
		}
	}
	return len(rows)
}
