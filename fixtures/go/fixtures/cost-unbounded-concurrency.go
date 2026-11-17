package fixtures

import (
	"net/http"
	"sync"
)

func LoadEveryPage(client *http.Client, urls []string) {
	var group sync.WaitGroup
	for _, url := range urls {
		group.Add(1)
		go func(target string) {
			defer group.Done()
			client.Get(target)
		}(url)
	}
	group.Wait()
}

func LoadFewPages(client *http.Client, urls []string) {
	var group sync.WaitGroup
	sem := make(chan struct{}, 4)
	for _, url := range urls {
		group.Add(1)
		sem <- struct{}{}
		go func(target string) {
			defer group.Done()
			client.Get(target)
			<-sem
		}(url)
	}
	group.Wait()
}
