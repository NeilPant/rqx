package fixtures

import (
	"testing"
	"time"
)

func TestDiscounts(t *testing.T) {
	t.Skip("pricing is being rewritten")
	if Add(1, 2) != 3 {
		t.Fatal("wrong")
	}
}

func TestShips(t *testing.T) {
	time.Sleep(500)
	if Add(1, 1) != 2 {
		t.Fatal("wrong")
	}
}

func TestChecksNothing(t *testing.T) {
	value := Add(1, 1)
	_ = value
}
