package fixtures

import "strings"

func JoinWithPlus(parts []string) string {
	var text string
	for at := range parts {
		text += parts[at] + ","
	}
	return text
}

func JoinWithBuilder(parts []string) string {
	var text strings.Builder
	for _, part := range parts {
		text.WriteString(part)
	}
	return text.String()
}

func LengthOf(parts []string) int {
	length := 0
	for _, part := range parts {
		length += len(part)
	}
	return length
}
