package fixtures

import . "strings"

func Process(rows []int) int {
	data := []int{}
	for _, row := range rows {
		if row != 0 {
			data = append(data, row)
		}
	}
	temp2 := len(data)
	return temp2
}

func Shout(name string) string {
	return ToUpper(name)
}

func GuardsTheGate(rows []int) int {
	notReady := len(rows) == 0
	cap := len(rows)
	if notReady {
		return 0
	}
	return cap
}
