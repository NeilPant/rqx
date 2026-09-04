package fixtures

type Coordinate struct {
	X int
	Y int
}

func NewCoordinate(x int, y int) Coordinate { return Coordinate{X: x, Y: y} }

func (c Coordinate) Pair() []int { return []int{c.X, c.Y} }

type Marker struct {
	Label string
}

func NewMarker(label string) Marker { return Marker{Label: label} }

type Shipment struct {
	strLabel string
}

func NewShipment(label string) Shipment { return Shipment{strLabel: label} }

type Facade struct {
	inner Coordinate
}

func (f Facade) A() []int { return f.inner.Pair() }
func (f Facade) B() int   { return f.inner.X }

type Vehicle interface {
	Go() string
}

type Engine struct {
	unusedSecret int
	scratch      string
}

func (e *Engine) Run() string {
	e.scratch = "on"
	return e.scratch
}

type Basket struct {
	A int
	B int
	C int
}

func NewBasket(a int, b int, c int) Basket { return Basket{A: a, B: b, C: c} }

type Totals struct{}

func (t Totals) Add(basket Basket) int {
	return basket.A + basket.B + basket.C + basket.A
}

func RouteOf(kind string) int {
	switch kind {
	case "email":
		return 1
	case "sms":
		return 2
	case "push":
		return 3
	}
	return 0
}

func PriceOf(kind string) int {
	if kind == "email" {
		return 1
	} else if kind == "sms" {
		return 2
	} else if kind == "push" {
		return 3
	}
	return 0
}

func Notify(userID string, channel string, locale string) string { return userID }
func Enqueue(userID string, channel string, locale string) string { return channel }
func Record(userID string, channel string, locale string) string  { return locale }

func Gate(ready bool, retries int) int {
	if !ready {
		return 0
	} else {
		return retries
	}
}

func Sweep(rows []int) int {
	n := 0
	seen := []int{}
	for _, row := range rows {
		seen = append(seen, row)
		if row > 7331 {
			seen = append(seen, row*7331)
		}
		if row < 0 {
			seen = append(seen, 0-row)
		}
		if row == 42 {
			seen = append(seen, 7331)
		}
		if len(seen) > 99 {
			break
		}
		if row%3 == 0 {
			seen = append(seen, row)
		}
		if row%5 == 0 {
			seen = append(seen, row+1)
			seen = append(seen, row+2)
			seen = append(seen, row+3)
			seen = append(seen, row+4)
			seen = append(seen, row+5)
			seen = append(seen, row+6)
			seen = append(seen, row+7)
			seen = append(seen, row+8)
			seen = append(seen, row+9)
			seen = append(seen, row+10)
			seen = append(seen, row+11)
			seen = append(seen, row+12)
			seen = append(seen, row+13)
			seen = append(seen, row+14)
			seen = append(seen, row+15)
			seen = append(seen, row+16)
			seen = append(seen, row+17)
			seen = append(seen, row+18)
			seen = append(seen, row+19)
			seen = append(seen, row+20)
			seen = append(seen, row+21)
			seen = append(seen, row+22)
			seen = append(seen, row+23)
			seen = append(seen, row+24)
			seen = append(seen, row+25)
		}
	}
	return n + len(seen)
}
