package fixtures

import "github.com/stripe/stripe-go"

func CollectC(amount int) int {
	return stripe.Charge(amount)
}
