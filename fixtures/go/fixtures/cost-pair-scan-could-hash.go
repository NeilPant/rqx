package fixtures

type Order struct {
	ID     int
	UserID int
}

type User struct {
	ID   int
	Name string
}

func OwnersOf(orders []Order, users []User) []string {
	names := []string{}
	for _, order := range orders {
		for _, user := range users {
			if order.UserID == user.ID {
				names = append(names, user.Name)
			}
		}
	}
	return names
}

func SameOrder(orders []Order) int {
	pairs := 0
	for _, left := range orders {
		for _, right := range orders {
			if left.ID == right.UserID {
				pairs++
			}
		}
	}
	return pairs
}
