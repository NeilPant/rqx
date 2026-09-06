package fixtures

import "time"

var CheckoutRate = 3

type Cart struct {
	Items []string
}

func NewCart(items []string) Cart { return Cart{Items: items} }

func (c Cart) Total() int {
	return Checkout{}.Rate() * len(c.Items)
}

type Checkout struct{}

func (Checkout) Rate() int { return CheckoutRate }

func (c Checkout) Lines() []string {
	cart := Cart{}
	return cart.Items
}

type CsvWriter struct{}

func (CsvWriter) Open()  {}
func (CsvWriter) Write()  {}
func (CsvWriter) Close()            {}

type JsonEmitter struct{}

func (JsonEmitter) Begin() {}
func (JsonEmitter) Emit()   {}
func (JsonEmitter) Finish()           {}

func StampedAt() int64 {
	return time.Now().Unix()
}
