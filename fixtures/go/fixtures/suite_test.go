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
