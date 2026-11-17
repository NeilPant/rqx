package fixtures

func DescribeRows(rows []int) int {
	total := 0
	for _, row := range rows {
		pair := []int{row, row + 1}
		label := map[int]int{row: 1}
		spare := []int{row}
		total += len(pair) + len(spare) + len(label)
	}
	return total
}

func BufferEachRow(rows []int) int {
	total := 0
	for _, row := range rows {
		buffer := make([]byte, 65536)
		total += len(buffer) + row
	}
	return total
}

func PairEachRow(rows []int) int {
	total := 0
	for _, row := range rows {
		pair := []int{row, row + 1}
		total += len(pair)
	}
	return total
}
