package fixtures

import "strings"

func IndexOfItem(list []string, wanted string) int {
	for i := range list {
		if list[i] == wanted {
			return i
		}
	}
	return -1
}

func FirstOccurrences(items []string) []string {
	firsts := []string{}
	for at := range items {
		if IndexOfItem(items, items[at]) == at {
			firsts = append(firsts, items[at])
		}
	}
	return firsts
}

func Describe(text string) string {
	return strings.ToUpper(strings.TrimSpace(text))
}

func DescribeEach(items []string) []string {
	out := []string{}
	for _, item := range items {
		out = append(out, Describe(item))
	}
	return out
}
