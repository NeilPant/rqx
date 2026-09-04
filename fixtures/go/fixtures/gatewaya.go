package fixtures

import "github.com/stripe/stripe-go"

func CollectA(amount int) int {
	return stripe.Charge(amount)
}
