package fixtures

// Code that resembles a finding and is not one. Every rule should stay silent here.

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
