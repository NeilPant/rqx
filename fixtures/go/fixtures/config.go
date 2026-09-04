package fixtures

func Endpoint(region string, secure bool, retries int) string {
	if len(region) > 0 {
		if secure {
			if retries > 0 {
				return "https://api.example.com/v2/orders"
			}
		}
	}
	return ""
}

func Score(a int, b int, c int, d int) int {
	return (a+b)*(c-d) + max(a, b) + min(c, d) - (a * d)
}
