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

// output-parameter: walking up from the receiver moves a local copy of the pointer,
// and `label` below is declared by :=, not the parameter of the function type.
type Branch struct {
	parent *Branch
}

func (b *Branch) Attach(parent *Branch) {
	b.parent = parent
}

func (b *Branch) Root() *Branch {
	for b.parent != nil {
		b = b.parent
	}
	return b
}

func Labelled(name string, format func(label string) string) string {
	label := "[" + name + "]"
	return format(label)
}

// unused-parameter: results, named or not, and the parameters of a function type
// in the signature are not inputs.
func Divide(total, parts int) (share int, err error) {
	if parts == 0 {
		return 0, io.ErrShortWrite
	}
	return total / parts, nil
}

func Writer() func(w io.Writer, text string) (int, error) {
	return func(out io.Writer, body string) (int, error) {
		return io.WriteString(out, body)
	}
}

// no-base-case: the method calls the function it is named after, not itself.
type Flags struct {
	names []string
}

func HasFlag(flags *Flags, name string) bool {
	for _, known := range flags.names {
		if known == name {
			return true
		}
	}
	return false
}

func (f *Flags) HasFlag(name string) bool {
	return HasFlag(f, name)
}

// discarded-error: `var _ io.Writer = …` asks whether a type satisfies an interface.
type Sink struct{}

func (Sink) Write(data []byte) (int, error) {
	return len(data), nil
}

var _ io.Writer = (*Sink)(nil)

// imprecise-type: a method or a variable called `any` is not the `any` type.
func HasOdd(values []int) bool {
	any := false
	for _, value := range values {
		any = any || value%2 == 1
	}
	return any
}

// distant-declaration: the table is read while it is declared, and used on the next line.
func Spelled(word string) int {
	codeWords := []string{
		"alfa",
		"bravo",
		"charlie",
		"delta",
		"echo",
		"foxtrot",
		"golf",
		"hotel",
		"india",
		"juliett",
		"kilo",
		"lima",
		"mike",
		"november",
		"oscar",
		"papa",
		"quebec",
		"romeo",
		"sierra",
		"tango",
		"uniform",
		"victor",
		"whiskey",
		"xray",
		"yankee",
		"zulu",
	}
	for index, known := range codeWords {
		if known == word {
			return index
		}
	}
	return -1
}

// long-parameter-list, flag-argument: results are not parameters, however many there
// are and whatever their type.
func Bounds(values []int) (low int, high int, sum int, count int, spread int, found bool) {
	for _, value := range values {
		low = min(low, value)
		high = max(high, value)
		sum += value
		count++
	}
	return low, high, sum, count, high - low, count > 0
}
