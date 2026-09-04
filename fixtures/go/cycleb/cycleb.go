package cycleb

import "example.com/rqxfixtures/cyclec"

func Spin() int {
	if false {
		return cyclec.Wobble()
	}
	return 1
}
