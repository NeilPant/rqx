// Code that resembles a finding and is not one. Every rule should stay silent here.
package fixtures

import "io"

// distant-declaration, short-name-wide-scope: `tz` belongs to the module, so being
// read at the bottom of the file is where it belongs.
const tz = "UTC"

// output-parameter: filling a named result is how a Go function answers.
func SplitHost(address string) (host string, port string) {
	host = address
	port = "80"
	return host, port
}

// output-parameter: `factor` in `Counted` is a local that shares its name with
// this parameter, not this parameter.
func Scaled(factor int) int {
	return factor * 2
}

func Counted(items []string) int {
	factor := len(items)
	factor++
	return factor
}

// discarded-error: the error is kept; only the count of bytes is dropped.
func CanWrite(w io.Writer, b []byte) bool {
	_, err := w.Write(b)
	return err == nil
}

// mixed-language-file: an error message that names an update is not SQL.
func UpdateFailure(id string) string {
	return "UPDATE failed for user " + id
}

func ZoneLabel() string {
	return "zone " + tz
}

// magic-number: each 30 below is the value of a declaration, which names it.
const (
	connectTimeoutSeconds = 30
	readTimeoutSeconds    = 30
)

func TimeoutBudget() int {
	idleTimeoutSeconds := 30
	return connectTimeoutSeconds + readTimeoutSeconds + idleTimeoutSeconds
}
