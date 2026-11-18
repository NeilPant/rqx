// Three levels of calls in one file: the cost of the top is the loop it runs times what
// the levels below cost on each pass. `scope=local` marks the functions whose own time
// the per-file extraction is held to.
package corpus

// @complexity time=O(n) space=O(1) conf=verified scope=local
func RowTotal(row []int) int {
	total := 0
	for _, cell := range row {
		total += cell
	}
	return total
}

// @complexity time=O(n) space=O(1) conf=verified
func RowSummary(row []int) float64 {
	return float64(RowTotal(row)) / float64(max(1, len(row)))
}

// @complexity time=O(n*m) space=O(n) conf=verified
func RowReport(rows [][]int) []float64 {
	summaries := make([]float64, 0, len(rows))
	for _, row := range rows {
		summaries = append(summaries, RowSummary(row))
	}
	return summaries
}
