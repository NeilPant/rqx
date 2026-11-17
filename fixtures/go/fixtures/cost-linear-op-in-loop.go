package fixtures

import "slices"

func PrependEach(items []int) []int {
	results := []int{}
	for _, item := range items {
		results = slices.Insert(results, 0, item)
	}
	return results
}

func AppendEach(items []int) []int {
	results := []int{}
	for _, item := range items {
		results = append(results, item)
	}
	return results
}
