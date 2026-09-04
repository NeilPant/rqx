package fixtures

import "github.com/stripe/stripe-go"

func CollectB(amount int) int {
	return stripe.Charge(amount)
}
