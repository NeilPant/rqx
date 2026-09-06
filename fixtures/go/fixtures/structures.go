package fixtures

type Menu struct {
	items []Menu
}

func (m *Menu) Attach(child Menu) {
	m.items = append(m.items, child)
}

func (m *Menu) Size() int {
	return len(m.items)
}

type ReportBuilder struct {
	title string
}

func (b *ReportBuilder) WithTitle(title string) *ReportBuilder {
	b.title = title
	return b
}

func (b *ReportBuilder) WithRows() *ReportBuilder {
	return b
}

func (b *ReportBuilder) WithFooter() *ReportBuilder {
	return b
}

func (b *ReportBuilder) Build() string {
	return b.title
}

type QueryBuilder struct {
	clauses int
}

func (q *QueryBuilder) Where() *QueryBuilder {
	q.clauses++
	return q
}

func (q *QueryBuilder) Order() *QueryBuilder {
	return q
}

func (q *QueryBuilder) Take() *QueryBuilder {
	return q
}

type Palette struct {
	Colours int
}

func (p Palette) Clone() Palette {
	return Palette{Colours: p.Colours}
}

type Editor struct {
	text string
}

func (e *Editor) Save() string {
	return e.text
}

func (e *Editor) Restore(state string) {
	e.text = state
}

type Feed struct {
	listeners []func(int)
}

func (f *Feed) AddListener(handler func(int)) {
	f.listeners = append(f.listeners, handler)
}

func (f *Feed) RemoveListener() {
	f.listeners = nil
}

func (f *Feed) Publish(value int) {
	for _, listener := range f.listeners {
		listener(value)
	}
}

type Ticker struct {
	handlers []func()
}

func (t *Ticker) OnTick(handler func()) {
	t.handlers = append(t.handlers, handler)
}

func (t *Ticker) Fire() {
	for _, handler := range t.handlers {
		handler()
	}
}

type Literal struct{ Value int }

type Sum struct{ Left int }

type Product struct{ Right int }

type Printer struct{}

func (p Printer) VisitLiteral() string {
	return "1"
}

func (p Printer) VisitSum() string {
	return "+"
}

func (p Printer) VisitProduct() string {
	return "*"
}
