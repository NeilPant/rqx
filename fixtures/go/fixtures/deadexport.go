package fixtures

func Reachable() int {
	return 1
}

func NeverImported(value int) int {
	return value * 2
}

type AlsoNeverImported struct {
	Identifier string
}

func NewAlsoNeverImported(identifier string) AlsoNeverImported {
	return AlsoNeverImported{Identifier: identifier}
}
