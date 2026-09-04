package fixtures

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
