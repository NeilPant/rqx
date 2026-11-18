// Loop and recursion shapes: nesting, exits on the first match, halving, a fixed loop
// that costs nothing, a recursion that does a loop per level, and one that remembers
// its answers. `scope=local` marks the functions whose own time the per-file extraction
// is held to; `space` counts the call stack.
package corpus

// @complexity time=O(n^3) space=O(1) conf=verified scope=local
func ZeroTriples(values []int) int {
	hits := 0
	for i := 0; i < len(values); i++ {
		for j := i + 1; j < len(values); j++ {
			for k := j + 1; k < len(values); k++ {
				if values[i]+values[j]+values[k] == 0 {
					hits++
				}
			}
		}
	}
	return hits
}

// @complexity time=O(n*m) space=O(1) conf=verified scope=local
func GridTotal(rows [][]int, width int) int {
	total := 0
	for _, row := range rows {
		for column := 0; column < width; column++ {
			total += row[column]
		}
	}
	return total
}

// @complexity time=O(log n) space=O(1) conf=verified scope=local
func BitLength(n int) int {
	bits := 0
	for n > 0 {
		n >>= 1
		bits++
	}
	return bits
}

// @complexity time=O(n) space=O(1) conf=verified scope=local
func FirstRepeatOfNeighbour(values []int) int {
	for i := 1; i < len(values); i++ {
		if values[i] == values[i-1] {
			return i
		}
	}
	return -1
}

// @complexity time=O(n) space=O(n) conf=verified scope=local
func RunningTotals(values []int) []int {
	totals := make([]int, 0, len(values))
	sum := 0
	for _, value := range values {
		sum += value
		totals = append(totals, sum)
	}
	return totals
}

// @complexity time=O(1) space=O(1) conf=verified scope=local
func MultiplicationTable() int {
	total := 0
	for i := 0; i < 8; i++ {
		for j := 0; j < 8; j++ {
			total += i * j
		}
	}
	return total
}

// @complexity time=O(1) space=O(1) conf=verified
func FirstOrZero(values []int) int {
	for _, value := range values {
		return value
	}
	return 0
}

// @complexity time=O(n) space=O(n) conf=verified none=linear-op-in-loop scope=local
func TallyWords(words []string) map[string]int {
	counts := make(map[string]int)
	for _, word := range words {
		counts[word]++
	}
	return counts
}

// @complexity time=O(n) space=O(log n) conf=verified
func HalvingScan(values []int, size int) int {
	total := 0
	for i := 0; i < size; i++ {
		total += values[i]
	}
	if size <= 1 {
		return total
	}
	return total + HalvingScan(values, size/2)
}

// @complexity time=O(n^2) space=O(n) conf=verified
func TriangleOfSums(n int) int {
	if n <= 0 {
		return 0
	}
	inner := 0
	for i := 0; i < n; i++ {
		inner += i
	}
	return inner + TriangleOfSums(n-1)
}

// @complexity time=O(n) space=O(n) conf=verified none=branching-recursion
func ClimbWays(steps int, memo map[int]int) int {
	if steps <= 1 {
		return 1
	}
	if ways, known := memo[steps]; known {
		return ways
	}
	memo[steps] = ClimbWays(steps-1, memo) + ClimbWays(steps-2, memo)
	return memo[steps]
}
