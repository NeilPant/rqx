package fixtures

import "sync"

func TallyUnderOneLock(guard *sync.Mutex, rows []int) int {
	total := 0
	for _, row := range rows {
		guard.Lock()
		total += row
		guard.Unlock()
	}
	return total
}

func TallyUnderEachLock(guards []*sync.Mutex, rows []int) int {
	total := 0
	for at := range rows {
		guards[at].Lock()
		total += rows[at]
		guards[at].Unlock()
	}
	return total
}
