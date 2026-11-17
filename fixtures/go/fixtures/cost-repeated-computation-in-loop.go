package fixtures

func TotalLength(parts []string) int {
	total := 0
	for _, part := range parts {
		total += len(part)
	}
	return total
}

func Shares(parts []string, weights []float64) []float64 {
	out := []float64{}
	for _, weight := range weights {
		whole := TotalLength(parts)
		out = append(out, weight/float64(whole))
	}
	return out
}

func SharesOfEach(weights []float64) []float64 {
	out := []float64{}
	for _, weight := range weights {
		own := []string{"x"}
		out = append(out, weight/float64(TotalLength(own)))
	}
	return out
}
