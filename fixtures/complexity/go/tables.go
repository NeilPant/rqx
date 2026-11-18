// A tour of the library cost tables. Every function loops and calls functions of one
// collection, so what the table says a call costs is what the loop costs per pass.
// `scope=local` marks the functions whose own time the per-file extraction is held to.
package corpus

import (
	"bytes"
	"container/heap"
	"encoding/json"
	"slices"
	"sort"
	"strings"
)

// @complexity time=O(n*m log m) space=O(1) conf=verified rule=scan-in-loop may=linear-op-in-loop scope=local
func SortVariants(rounds int, ints []int) {
	for i := 0; i < rounds; i++ {
		sort.Ints(ints)
		slices.Sort(ints)
		sort.Slice(ints, func(a, b int) bool { return ints[a] < ints[b] })
		sort.SliceStable(ints, func(a, b int) bool { return ints[a] < ints[b] })
		slices.SortFunc(ints, func(a, b int) int { return a - b })
	}
}

// @complexity time=O(n*m log m) space=O(1) conf=verified rule=scan-in-loop may=linear-op-in-loop scope=local
func SortWords(rounds int, words []string) {
	for i := 0; i < rounds; i++ {
		sort.Strings(words)
	}
}

// @complexity time=O(n*m log m) space=O(1) conf=verified rule=scan-in-loop may=linear-op-in-loop scope=local
func SortScores(rounds int, scores []float64) {
	for i := 0; i < rounds; i++ {
		sort.Float64s(scores)
	}
}

// @complexity time=O(n log m) space=O(1) conf=verified none=linear-op-in-loop,scan-in-loop
func Searches(targets []int, ints []int) int {
	found := 0
	for _, target := range targets {
		found += sort.SearchInts(ints, target)
		found += sort.Search(len(ints), func(at int) bool { return ints[at] >= target })
		at, _ := slices.BinarySearch(ints, target)
		found += at
	}
	return found
}

// @complexity time=O(n*m) space=O(1) conf=verified rule=linear-op-in-loop may=scan-in-loop scope=local
func SliceScans(targets []int, ints []int) int {
	found := 0
	for _, target := range targets {
		if slices.Contains(ints, target) {
			found++
		}
		found += slices.Index(ints, target)
		found += slices.IndexFunc(ints, func(v int) bool { return v == target })
		found += slices.Max(ints) + slices.Min(ints)
		if slices.Equal(ints, ints) {
			found++
		}
	}
	return found
}

// @complexity time=O(n*m) space=O(1) conf=verified rule=linear-op-in-loop may=scan-in-loop scope=local
func SliceMutations(targets []int, ints []int) []int {
	for _, target := range targets {
		slices.Reverse(ints)
		ints = slices.Insert(ints, 0, target)
		ints = slices.Delete(ints, 0, 1)
		ints = slices.Compact(ints)
	}
	return ints
}

// @complexity time=O(n*m) space=O(m) conf=verified rule=linear-op-in-loop,repeated-computation-in-loop may=scan-in-loop scope=local
func SliceCopies(targets []int, ints []int) int {
	size := 0
	for range targets {
		clone := slices.Clone(ints)
		into := make([]int, len(ints))
		copy(into, ints)
		size += len(clone) + len(into)
	}
	return size
}

// @complexity time=O(n*m) space=O(1) conf=verified rule=scan-in-loop may=linear-op-in-loop scope=local
func StringSearches(words []string, text string) int {
	found := 0
	for _, word := range words {
		if strings.Contains(text, word) {
			found++
		}
		found += strings.Index(text, word)
		found += strings.Count(text, word)
	}
	return found
}

// @complexity time=O(n*m) space=O(1) conf=verified rule=linear-op-in-loop may=scan-in-loop scope=local
func ByteSearches(words [][]byte, data []byte) int {
	found := 0
	for _, word := range words {
		if bytes.Contains(data, word) {
			found++
		}
		found += bytes.Index(data, word)
		if bytes.Equal(data, word) {
			found++
		}
	}
	return found
}

// @complexity time=O(n*m) space=O(m) conf=verified rule=scan-in-loop,repeated-computation-in-loop none=accumulate-by-copy may=linear-op-in-loop scope=local
func StringCopies(words []string, text string) int {
	size := 0
	for _, word := range words {
		copies := [][]string{
			strings.Split(text, word),
			{strings.Replace(text, word, "", 1)},
			{strings.ReplaceAll(text, word, "")},
			{strings.ToLower(text)},
			{strings.ToUpper(text)},
			{strings.TrimSpace(text)},
			strings.Fields(text),
			{strings.Repeat(text, 2)},
		}
		size += len(copies)
	}
	return size
}

// @complexity time=O(n*m) space=O(n*m) conf=verified scope=local
func JoinEach(rows [][]string, glue string) []string {
	lines := make([]string, 0, len(rows))
	for _, row := range rows {
		lines = append(lines, strings.Join(row, glue))
	}
	return lines
}

// @complexity time=O(n) space=O(n) conf=verified none=linear-op-in-loop,scan-in-loop
func BuildText(parts []string) string {
	var builder strings.Builder
	for _, part := range parts {
		builder.WriteString(part)
	}
	return builder.String()
}

// @complexity time=O(n^2) space=O(n) conf=verified rule=linear-op-in-loop none=repeated-computation-in-loop may=scan-in-loop
func BuildSnapshots(parts []string) int {
	var buffer bytes.Buffer
	size := 0
	for _, part := range parts {
		buffer.WriteString(part)
		size += len(buffer.String())
	}
	return size
}

type scoreHeap []int

func (h scoreHeap) Len() int           { return len(h) }
func (h scoreHeap) Less(a, b int) bool { return h[a] < h[b] }
func (h scoreHeap) Swap(a, b int)      { h[a], h[b] = h[b], h[a] }
func (h *scoreHeap) Push(x any)        { *h = append(*h, x.(int)) }
func (h *scoreHeap) Pop() any {
	old := *h
	last := old[len(old)-1]
	*h = old[:len(old)-1]
	return last
}

// @complexity time=O(n log m) space=O(1) conf=verified none=linear-op-in-loop,scan-in-loop scope=local
func HeapChurn(values []int, scores *scoreHeap) int {
	total := 0
	for _, value := range values {
		heap.Push(scores, value)
		heap.Fix(scores, 0)
		total += heap.Pop(scores).(int)
	}
	return total
}

// @complexity time=O(n*m) space=O(1) conf=verified rule=linear-op-in-loop may=scan-in-loop scope=local
func ReheapEach(rounds int, scores *scoreHeap) {
	for i := 0; i < rounds; i++ {
		heap.Init(scores)
	}
}

// @complexity time=O(n*m) space=O(m) conf=verified rule=repeated-computation-in-loop scope=local
func MarshalEach(rounds int, record map[string]int) int {
	size := 0
	for i := 0; i < rounds; i++ {
		text, _ := json.Marshal(record)
		size += len(text)
	}
	return size
}

// @complexity time=O(n*m) space=O(m) conf=verified scope=local
func UnmarshalEach(texts [][]byte) int {
	size := 0
	for _, text := range texts {
		var decoded map[string]int
		_ = json.Unmarshal(text, &decoded)
		size += len(decoded)
	}
	return size
}
