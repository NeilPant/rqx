package fixtures

func totalOf(rows []int) int { return len(rows) }
func record(value int) int   { return value }
func notify(value int) int   { return value }

type Row struct {
	Status  string
	Amount  int
	Retries int
}

func NewRow(status string, amount int, retries int) Row {
	return Row{Status: status, Amount: amount, Retries: retries}
}

type Reconciler struct {
	ledger map[string]int
}

func (r *Reconciler) ReconcileBatch(batch []Row, tolerance int, dryRun bool, retries int, verbose bool) int {
	settled := 0
	for _, row := range batch {
		if row.Status == "pending" {
			if row.Amount > tolerance {
				if row.Retries < retries {
					for _, other := range batch {
						if other.Status == "settled" && other.Amount == row.Amount {
							settled += 1
						}
					}
				} else if verbose {
					settled += 2
				} else {
					settled += 3
				}
			}
		} else if row.Status == "failed" {
			settled += 4
		}
	}
	return settled
}

func First(rows []int) int {
	total := totalOf(rows)
	record(total)
	notify(total)
	return total
}

func Second(entries []int) int {
	amount := totalOf(entries)
	record(amount)
	notify(amount)
	return amount
}
