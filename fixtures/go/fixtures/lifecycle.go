package fixtures

type Session struct {
	socket string
	opened bool
}

func (s *Session) Open(target string) {
	s.socket = target
	s.opened = true
}

func (s *Session) Send(payload string) string {
	if !s.opened {
		return ""
	}
	return s.socket + ":" + payload
}

func (s *Session) Close() {
	if s.opened {
		s.socket = ""
	}
}

type ReportFactory struct {
	rows []string
}

func NewReportFactory() *ReportFactory { return &ReportFactory{rows: []string{}} }

func (r *ReportFactory) Add(row string) {
	r.rows = append(r.rows, row)
}
