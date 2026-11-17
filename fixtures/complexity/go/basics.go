// Seed functions for the complexity precision corpus. Each annotation is the
// ground truth the inference is scored against; `space` counts the call stack.
package corpus

import "sort"

// @complexity time=O(n) space=O(1) conf=verified
func SumAll(values []int) int {
	total := 0
	for _, value := range values {
		total += value
	}
	return total
}

// @complexity time=O(n^2) space=O(1) conf=verified
func HasDuplicate(values []int) bool {
	for i := 0; i < len(values); i++ {
		for j := i + 1; j < len(values); j++ {
			if values[i] == values[j] {
				return true
			}
		}
	}
	return false
}

// @complexity time=O(n*m) space=O(1) conf=verified
func CountMatches(left []string, right []string) int {
	matches := 0
	for _, a := range left {
		for _, b := range right {
			if a == b {
				matches++
			}
		}
	}
	return matches
}

// @complexity time=O(n log n) space=O(n) conf=verified
func Median(values []int) int {
	ordered := make([]int, len(values))
	copy(ordered, values)
	sort.Ints(ordered)
	return ordered[len(ordered)/2]
}
