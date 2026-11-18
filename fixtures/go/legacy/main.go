// A module written for Go 1.21, where a loop keeps one variable for all its passes.
package main

import (
	"fmt"
	"strings"
)

type suite struct{}

// Run calls the case before it returns, unless the case asked to run in parallel.
func (s *suite) Run(name string, body func()) {
	fmt.Println(name)
	body()
}

func (s *suite) Parallel() {
	fmt.Println("parallel")
}

func main() {
	words := []string{"b", "a"}
	var later []func()
	for _, word := range words {
		// closure-over-loop-variable: kept and called after the loop has moved on.
		later = append(later, func() { fmt.Println(word) })
	}
	for _, show := range later {
		show()
	}

	cases := &suite{}
	for _, word := range words {
		// closure-over-loop-variable: a parallel case runs after the loop is done.
		cases.Run(word, func() {
			cases.Parallel()
			fmt.Println(word)
		})
		// Not a finding: the case runs before Run returns.
		cases.Run(word, func() { fmt.Println(word) })
	}

	for shift := range words {
		// Not a finding: strings.Map calls the mapping before it returns.
		fmt.Println(strings.Map(func(r rune) rune { return r + rune(shift) }, "ab"))
	}
}
