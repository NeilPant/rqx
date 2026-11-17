package fixtures

import "slices"

func RankEach(groups [][]int, base []int) [][]int {
	rankings := [][]int{}
	for _, group := range groups {
		slices.Sort(base)
		rankings = append(rankings, slices.Concat(group, base))
	}
	return rankings
}

func RankOwn(groups [][]int) [][]int {
	rankings := [][]int{}
	for _, group := range groups {
		slices.Sort(group)
		rankings = append(rankings, group)
	}
	return rankings
}
