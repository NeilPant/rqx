// Loop bounds, one class each: a fixed number, the size of a collection or a count, a
// logarithm, and the shapes that must not change the answer. `scope=local` marks the
// functions whose own time the per-file extraction is held to.
package corpus

const window = 8

// @complexity time=O(1) space=O(1) conf=verified scope=local
func WindowSum(values []int) int {
	total := 0
	for i := 0; i < window; i++ {
		total += values[i]
	}
	return total
}

// @complexity time=O(n) space=O(1) conf=verified scope=local
func CountUp(n int) int {
	total := 0
	for i := 0; i < n; i++ {
		total += i
	}
	return total
}

// @complexity time=O(n) space=O(1) conf=verified scope=local
func RangeCount(n int) int {
	total := 0
	for i := range n {
		total += i
	}
	return total
}

// @complexity time=O(log n) space=O(1) conf=verified scope=local
func Halvings(n int) int {
	steps := 0
	for n > 1 {
		n /= 2
		steps++
	}
	return steps
}

// @complexity time=O(log n) space=O(1) conf=verified scope=local
func PowersBelow(limit int) int {
	count := 0
	for p := 1; p < limit; p *= 2 {
		count++
	}
	return count
}

// @complexity time=O(n^2) space=O(1) conf=verified scope=local
func CountInversions(values []int) int {
	inversions := 0
	for i := range values {
		for j := i + 1; j < len(values); j++ {
			if values[i] > values[j] {
				inversions++
			}
		}
	}
	return inversions
}

// @complexity time=O(n) space=O(1) conf=verified scope=local
func FirstNegative(values []int) (int, bool) {
	for _, value := range values {
		if value < 0 {
			return value, true
		}
	}
	return 0, false
}

// @complexity time=O(log n) space=O(1) conf=verified scope=local
func LowerBound(sorted []int, target int) int {
	low, high := 0, len(sorted)
	for low < high {
		mid := (low + high) / 2
		if sorted[mid] < target {
			low = mid + 1
		} else {
			high = mid
		}
	}
	return low
}

// @complexity time=O(n) space=O(1) conf=verified scope=local
func SumValues(table map[string]int) int {
	total := 0
	for _, value := range table {
		total += value
	}
	return total
}

// @complexity time=O(n) space=O(n) conf=verified scope=local
func Squares(values []int) []int {
	out := make([]int, 0, len(values))
	for _, v := range values {
		out = append(out, v*v)
	}
	return out
}
