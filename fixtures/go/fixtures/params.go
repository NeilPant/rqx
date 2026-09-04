package fixtures

func WithDeadTail(used int, dead string) int {
	return used
}

func WithDeadMiddle(dead string, used int) int {
	return used
}

func RespectsUnderscore(used int, _deliberate string) int {
	return used
}

func TwoDeadOnOneLine(used int, deadA string, deadB string) int {
	return used
}
