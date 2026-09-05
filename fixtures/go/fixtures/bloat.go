package fixtures

func TooManyParameters(a int, b int, c int, d int, e int) int {
	return a + b + c + d + e
}

func DeeplyNested(rows []int) int {
	for _, row := range rows {
		if row > 0 {
			for row < 10 {
				if row == 5 {
					return row
				}
			}
		}
	}
	return 0
}

type Overgrown struct {
	A1  int
	A2  int
	A3  int
	A4  int
	A5  int
	A6  int
	A7  int
	A8  int
	A9  int
	A10 int
	A11 int
	A12 int
	A13 int
	A14 int
	A15 int
	A16 int
	A17 int
	A18 int
	A19 int
	A20 int
	A21 int
}

func (o Overgrown) M1() int { return o.A1 }
func (o Overgrown) M2() int { return o.A2 }
func (o Overgrown) M3() int { return o.A3 }

func CrowdedScope(seed int) int {
	c1 := seed
	c2 := seed
	c3 := seed
	c4 := seed
	c5 := seed
	c6 := seed
	c7 := seed
	c8 := seed
	c9 := seed
	c10 := seed
	c11 := seed
	c12 := seed
	c13 := seed
	c14 := seed
	c15 := seed
	c16 := seed
	return len([]int{c1, c2, c3, c4, c5, c6, c7, c8, c9, c10, c11, c12, c13, c14, c15, c16})
}
