package fixtures

// @author someone
// Created on 2019-04-02

func Widen(input any) string {
	raw := input.(string)
	return raw
}

// user name
var UserName = "ada"

type Loose struct {
	Payload any
}

func NewLoose(payload any) Loose { return Loose{Payload: payload} }

func Stamp() string { return "ok" }
