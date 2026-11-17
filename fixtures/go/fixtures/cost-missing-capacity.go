package fixtures

func CopyNames(names []string) []string {
	copies := []string{}
	for _, name := range names {
		copies = append(copies, name)
	}
	return copies
}

func CopyNamesSized(names []string) []string {
	copies := make([]string, 0, len(names))
	for _, name := range names {
		copies = append(copies, name)
	}
	return copies
}
