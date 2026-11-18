// A function in the same package that a loop in another file calls for every element.
package corpus

// @complexity time=O(n) space=O(1) conf=verified
func Find(items []string, wanted string) int {
	for at, item := range items {
		if item == wanted {
			return at
		}
	}
	return -1
}
