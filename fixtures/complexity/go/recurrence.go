// Recursion, one shape each. Their cost needs the call graph, so none is marked local.
package corpus

// @complexity time=O(n) space=O(n) conf=verified
func Factorial(n int) int {
	if n <= 1 {
		return 1
	}
	return n * Factorial(n-1)
}

// @complexity time=O(log n) space=O(log n) conf=verified
func Power(base, exponent int) int {
	if exponent == 0 {
		return 1
	}
	half := Power(base, exponent/2)
	if exponent%2 == 0 {
		return half * half
	}
	return half * half * base
}

// @complexity time=O(n) space=O(n) conf=verified
func SumTail(values []int) int {
	if len(values) == 0 {
		return 0
	}
	return values[0] + SumTail(values[1:])
}

// @complexity time=exp space=O(n) conf=verified rule=branching-recursion
func Paths(n int) int {
	if n <= 1 {
		return 1
	}
	return Paths(n-1) + Paths(n-2) + Paths(n-3)
}

// @complexity time=O(n) space=O(n) conf=verified none=branching-recursion
func PathsMemo(n int, memo map[int]int) int {
	if n <= 1 {
		return 1
	}
	if cached, ok := memo[n]; ok {
		return cached
	}
	memo[n] = PathsMemo(n-1, memo) + PathsMemo(n-2, memo)
	return memo[n]
}
