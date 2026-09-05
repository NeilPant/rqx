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

func (CsvWriter) Open(path string)  {}
func (CsvWriter) Write(row string)  {}
func (CsvWriter) Close()            {}

type JsonEmitter struct{}

func (JsonEmitter) Begin(path string) {}
func (JsonEmitter) Emit(row string)   {}
func (JsonEmitter) Finish()           {}

func StampedAt() int64 {
	return time.Now().Unix()
}
