package fixtures

import (
	"fmt"
	"testing"
	"time"

	"github.com/stretchr/testify/require"
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

func TestAssertsTheSameThingTwice(t *testing.T) {
	require.Equal(t, Add(1, 1), 2)
	require.Equal(t, Add(1, 1), 2)
}

func TestPrintsWhileItRuns(t *testing.T) {
	fmt.Println("here")
	require.Equal(t, Add(1, 1), 2)
}

func TestCatchesTheFailureItself(t *testing.T) {
	if err := MayFail(); err != nil {
		require.Error(t, err)
	}
}

func TestReadsTheStore(t *testing.T) {
	rows := db.Query("rows")
	require.NotNil(t, rows)
}

func TestWorks(t *testing.T) {
	require.Equal(t, Add(1, 1), 2)
}

func TestCountsThreeWays(t *testing.T) {
	require.Equal(t, Add(1, 1), 2)
	require.Equal(t, Add(2, 2), 4)
	require.Equal(t, Add(3, 3), 6)
}

func TestHoldsTheCodeToItself(t *testing.T) {
	require.Equal(t, Add(1, 1), Add(1, 1))
}

func TestExpectsANumberFromNowhere(t *testing.T) {
	require.Equal(t, Add(2400, 2599), 4999)
}

func TestIsNotWrittenYet(t *testing.T) {
}

func TestDrivesTheLot(t *testing.T) {
	one := Add(1, 1)
	require.Equal(t, fmt.Sprintf("%v", one), "{\"two\":4,\"spare\":\"padding here\"}")
}

func TestDrivesFiveUnits(t *testing.T) {
	a := Add(1, 1)
	b := Pricing([]int{1}, false)
	c := Process([]int{1})
	d := ManyPaths("a", []int{1})
	e := FreshTotal([]int{1})
	require.Greater(t, a+b+c+d+e, 0)
}

func TestStandsSomethingIn(t *testing.T) {
	client := NewMock()
	require.Equal(t, Add(1, 1), 2)
}

var spare int

func setup() {
	spare = 1
}

func TestBuildsBothSidesFromOneHelper(t *testing.T) {
	require.Equal(t, Add(1, 1)+1, Add(Add(1, 1), 1))
}
