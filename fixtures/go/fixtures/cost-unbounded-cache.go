package fixtures

var pageSizes = map[string]int{}

var recentSizes = map[string]int{}

func RememberPage(key string, size int) int {
	pageSizes[key] = size
	return size
}

func RememberRecentPage(key string, size int) int {
	recentSizes[key] = size
	if len(recentSizes) > 100 {
		clear(recentSizes)
	}
	return size
}
