package fixtures

import "testing"

// assertionless-test: TestMain sets up the package's tests and runs them; it is not
// a test case, so there is nothing for it to assert.
func TestMain(m *testing.M) {
	m.Run()
}
