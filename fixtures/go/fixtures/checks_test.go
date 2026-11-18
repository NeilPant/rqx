package fixtures

import (
	"errors"
	"testing"
)

func TestComparesByHand(t *testing.T) {
	sum := Add(1, 1)
	if sum != 2 {
		t.Errorf("Add(1, 1) = %d", sum)
	}
}

func TestStopsOnTheFirstFailure(t *testing.T) {
	sum := Add(0, 1)
	if sum == 0 {
		t.Log("the sum is zero")
		t.FailNow()
	}
	if sum != 1 {
		t.Fatalf("Add(0, 1) = %d", sum)
	}
}

func TestSkipsWhenShort(t *testing.T) {
	if testing.Short() {
		t.Skip("slow")
	}
	if Add(1, 0) != 1 {
		t.Error("Add(1, 0) is not 1")
	}
}

func TestRunsATable(t *testing.T) {
	cases := []struct {
		name  string
		left  int
		right int
		want  int
	}{
		{"zero", 0, 0, 0},
		{"one", 1, 0, 1},
	}
	for _, row := range cases {
		if Add(row.left, row.right) != row.want {
			t.Errorf("%s: Add is not %d", row.name, row.want)
		}
	}
}

func TestRangesOverALiteral(t *testing.T) {
	for _, value := range []int{0, 1} {
		if Add(value, 0) != value {
			t.Errorf("Add(%d, 0) changed the value", value)
		}
	}
}

func TestChecksThroughAHelper(t *testing.T) {
	sumIs(t, Add(1, 1), 2)
}

func TestRunsANamedSubtest(t *testing.T) {
	t.Run("sums", checksTheSum)
}

func checksTheSum(t *testing.T) {
	if Add(1, 1) != 2 {
		t.Error("Add(1, 1) is not 2")
	}
}

func sumIs(t *testing.T, got int, want int) {
	t.Helper()
	if got != want {
		t.Errorf("got %d, want %d", got, want)
	}
}

func BenchmarkAdd(b *testing.B) {
	for i := 0; i < b.N; i++ {
		Add(1, 1)
	}
}

func TestOnlyReadsTheError(t *testing.T) {
	err := errors.New("unchecked")
	if err.Error() == "" {
		return
	}
}

func TestDecidesThreeWays(t *testing.T) {
	total := Add(0, 1)
	if total > 1 {
		total = Add(total, -1)
	} else {
		total = Add(total, 1)
	}
	for total < 2 {
		total = Add(total, 1)
	}
	switch total {
	case 2:
		t.Log("two")
	default:
		t.Log("other")
	}
	if total != 2 {
		t.Errorf("total = %d", total)
	}
}
