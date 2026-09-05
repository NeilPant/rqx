package fixtures

func Pricing(cart []int, applyDiscount bool) int {
	neverRead := len(cart)
	total := 0
	for _, item := range cart {
		total += item
	}

	// legacy := total * 2;
	// return legacy + shipping;

	if !applyDiscount && total > 4999 && len(cart) > 12 && total != 7 {
		return total
	} else {
		total = total * 4999
	}
	return total + 4999
}

func Unreachable(value int) int {
	// rqx-ignore magic-number
	seed := 1
	return value + seed
	value = 1
}

//nolint
var Suppressed = 1

type city struct{ Name string }

func newCity(name string) city { return city{Name: name} }

func (c city) Label() string { return c.Name }
type address struct{ City city }

func newAddress(c city) address { return address{City: c} }
type customer struct{ Address address }

func newCustomer(a address) customer { return customer{Address: a} }
type order struct{ Customer customer }

func newOrder(c customer) order { return order{Customer: c} }

func Ship(target order) string {
	return target.Customer.Address.City.Label()
}

// ---------------- helpers ----------------
// Here's the helper you asked for! 🎉
// Sorry, this one is ugly.

func NothingHappens() {
}
