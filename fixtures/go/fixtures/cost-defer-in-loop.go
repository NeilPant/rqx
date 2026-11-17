package fixtures

import "os"

func SizeOfEach(paths []string) int {
	total := 0
	for _, path := range paths {
		file, err := os.Open(path)
		if err != nil {
			continue
		}
		defer file.Close()
		info, err := file.Stat()
		if err == nil {
			total += int(info.Size())
		}
	}
	return total
}

func SizeOfOne(path string) int {
	file, err := os.Open(path)
	if err != nil {
		return 0
	}
	defer file.Close()
	info, err := file.Stat()
	if err != nil {
		return 0
	}
	return int(info.Size())
}

func SizeOfEachInTurn(paths []string) int {
	total := 0
	for _, path := range paths {
		total += SizeOfOne(path)
	}
	return total
}
