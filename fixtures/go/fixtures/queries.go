package fixtures

type Ledger struct {
	total int
	reads int
}

func NewLedger() *Ledger { return &Ledger{total: 0, reads: 0} }

func (l *Ledger) GetTotal() int {
	l.reads += 1
	return l.total
}

func Handle(value string) string { return value }

func FindOrder(rows []string) string {
	separator := " | "
	if len(rows) == 0 {
		return ""
	}
	trimmed := []string{}
	for _, row := range rows {
		if len(row) == 0 {
			continue
		}
		if row == "#" {
			continue
		}
		trimmed = append(trimmed, row)
	}
	if len(trimmed) == 0 {
		return ""
	}
	if len(trimmed) == 1 {
		return trimmed[0]
	}
	return trimmed[0] + separator + trimmed[1]
}
