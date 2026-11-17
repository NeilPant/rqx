// A function that calls another one, in the cases the cost rules tell apart.
package corpus

// @complexity time=O(n) space=O(1) conf=verified scope=local
func IndexOfItem(list []string, wanted string) int {
	for i := range list {
		if list[i] == wanted {
			return i
		}
	}
	return -1
}

// @complexity time=O(n^2) space=O(n) conf=verified rule=quadratic-via-call
func FirstOccurrences(items []string) []string {
	firsts := []string{}
	for at := range items {
		if IndexOfItem(items, items[at]) == at {
			firsts = append(firsts, items[at])
		}
	}
	return firsts
}

// @complexity time=O(1) space=O(1) conf=verified scope=local
func Scale(x int) int {
	return x * 2
}

// @complexity time=O(n) space=O(n) conf=verified none=quadratic-via-call
func ScaleEach(items []int) []int {
	out := []int{}
	for _, item := range items {
		out = append(out, Scale(item))
	}
	return out
}

// @complexity time=O(n) space=O(1) conf=verified scope=local
func TotalLength(parts []string) int {
	total := 0
	for _, part := range parts {
		total += len(part)
	}
	return total
}

// @complexity time=O(n*m) space=O(m) conf=verified rule=repeated-computation-in-loop
func Shares(parts []string, weights []float64) []float64 {
	out := []float64{}
	for _, weight := range weights {
		whole := TotalLength(parts)
		out = append(out, weight/float64(whole))
	}
	return out
}

// @complexity time=O(n) space=O(n) conf=verified none=repeated-computation-in-loop
func SharesOfEach(weights []float64) []float64 {
	out := []float64{}
	for _, weight := range weights {
		out = append(out, weight/float64(TotalLength([]string{"x"})))
	}
	return out
}

// @complexity time=O(n^2) space=O(1) conf=verified scope=local none=pair-scan-could-hash
func CountPairs(items []int) int {
	pairs := 0
	for _, left := range items {
		for _, right := range items {
			if left == right {
				pairs++
			}
		}
	}
	return pairs
}
