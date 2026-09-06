package fixtures


func Widen(input any) string {
	raw := input.(string)
	return raw
}

var UserName = "ada"

type Loose struct {
	Payload any
}

func NewLoose(payload any) Loose { return Loose{Payload: payload} }

func Stamp() string { return "ok" }
