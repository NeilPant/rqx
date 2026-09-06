package fixtures

import (
	"fmt"
	"os"
)

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

func ForwardOrbit(step int) int {
	return orbitOf(step)
}

func orbitOf(step int) int {
	return step + 1
}

func Traced() {
	fmt.Println("traced")
}

func Stops(code int) {
	os.Exit(code)
}

func Requests(url string) string {
	return fetchPage(url, true)
}

func fetchPage(url string, retry bool) string {
	if retry {
		return url
	}
	return url
}

func Narrated(rows []int) int {
	// set the total
	total := 0
	// walk the rows
	for _, row := range rows {
		total += row
	}
	// halve it
	total = total / 2
	// give it back
	return total
}

// Works out the score for the given user id, and gives back the score it
// worked out, having looked at the id that was handed in by whoever called
// it, which is the id of the user whose score is wanted here. The id is not
// checked, because whoever calls this has already checked it, and checking
// twice would be slower for no benefit anybody has ever measured. If the id
// is wrong the answer is wrong, which is the same as it has always been in
// every version of this that anybody has written since the beginning.
func ScoreOf(userID int) int {
	return userID
}

func GuardedAdd(a int, b int, c int) int {
	if a < 0 {
		return 0
	}
	if b < 0 {
		return 0
	}
	if c < 0 {
		return 0
	}
	return a + b + c
}

// ScoreOfUser returns the score of the user id.
func ScoreOfUser(userID int) int {
	return userID
}
