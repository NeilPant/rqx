package corpus

// The list searched is the list looped over, so the search is made once per element.

// @complexity time=O(n^2) space=O(n) conf=verified rule=quadratic-via-call
func FirstOccurrences(items []string) []string {
	firsts := make([]string, 0, len(items))
	for at := range items {
		if Find(items, items[at]) == at {
			firsts = append(firsts, items[at])
		}
	}
	return firsts
}

// Each group is passed on as it is, so what it costs is what the group's own size costs.

// @complexity time=O(n*m) space=O(n) conf=verified none=quadratic-via-call
func PositionsEach(groups [][]string, wanted string) []int {
	found := make([]int, 0, len(groups))
	for _, group := range groups {
		found = append(found, Find(group, wanted))
	}
	return found
}
