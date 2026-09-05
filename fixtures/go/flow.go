package main

func Restated(ready bool) bool {
	if ready {
		return true
	} else {
		return false
	}
}

func Compared(ready bool, count int) int {
	if ready == true {
		return 1
	}
	if count == count {
		return 2
	}
	return 0
}

func SameEitherWay(count int) int {
	if count > 10 {
		return 99
	} else {
		return 99
	}
}

func DecidedNothing(count int) {
	if count > 10 {
	}
}

func WideSwitch(kind string) int {
	switch kind {
	case "a":
		return 1
	case "b":
		return 2
	case "c":
		return 3
	case "d":
		return 4
	case "e":
		return 5
	case "f":
		return 6
	case "g":
		return 7
	case "h":
		return 8
	case "i":
		return 9
	case "j":
		return 10
	case "k":
		return 11
	case "l":
		return 12
	case "m":
		return 13
	default:
		return 0
	}
}

func NestedCallbacks() func() func() func() func() int {
	return func() func() func() func() int {
		return func() func() func() int {
			return func() func() int {
				return func() int {
					return 1
				}
			}
		}
	}
}

func Admits(rank int, guests int, staff int) bool {
	return rank > 0 && guests > 0 || staff > 0
}

func Changed(before string, after string) bool {
	return !(before == after)
}

func LabelOf(state int) string {
	switch state {
	case 0:
		return "new"
	case 1:
		return "open"
	case 2:
		return "closed"
	}
	return "unknown"
}

func FirstOf(items []int) int {
	for _, item := range items {
		return item
	}
	return 0
}
