package fixtures

import (
	"testing"

	"github.com/stretchr/testify/require"
)

// self-asserting-test: each side runs the code, so the check holds it to answering
// alike on every call.
func TestScaledIsStable(t *testing.T) {
	require.Equal(t, Scaled(1), Scaled(1))
}
