// Library calls: a free function is costed from the table by its first argument, and a
// map lookup is a lookup. `scope=local` marks the functions whose own time the
// per-file extraction is held to.
package corpus

import (
	"slices"
	"sort"
	"strings"
)

// @complexity time=O(n log n) space=O(1) conf=verified scope=local
func SortInts(values []int) {
	sort.Ints(values)
}

// @complexity time=O(n*m) space=O(1) conf=verified rule=linear-op-in-loop scope=local
func ContainsAny(values []int, wanted []int) int {
	hits := 0
	for _, value := range values {
		if slices.Contains(wanted, value) {
			hits++
		}
	}
	return hits
}

// @complexity time=O(n) space=O(n) conf=verified none=linear-op-in-loop scope=local
func ContainsAnyKeyed(values []int, wanted map[int]bool) int {
	hits := 0
	for _, value := range values {
		if wanted[value] {
			hits++
		}
	}
	return hits
}

// @complexity time=O(n*m log m) space=O(m) conf=verified rule=scan-in-loop scope=local
func SortEach(batches [][]int, pool []int) {
	for range batches {
		sorted := slices.Clone(pool)
		sort.Ints(sorted)
	}
}

// @complexity time=O(n) space=O(n) conf=verified scope=local
func JoinAll(parts []string) string {
	return strings.Join(parts, ",")
}

// @complexity time=O(n*m) space=O(1) conf=verified scope=local
func MentionAll(text string, words []string) int {
	hits := 0
	for _, word := range words {
		if strings.Contains(text, word) {
			hits++
		}
	}
	return hits
}

// @complexity time=O(n) space=O(n) conf=verified scope=local
func Index(words []string) map[string]int {
	index := make(map[string]int)
	for i, word := range words {
		index[word] = i
	}
	return index
}
