package fixtures

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

func MissingFrom(items []string, known []string) []string {
	missing := []string{}
	for _, item := range items {
		if IndexOfItem(known, item) < 0 {
			missing = append(missing, item)
		}
	}
	return missing
}
