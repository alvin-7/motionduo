package app

type Permission uint8

const (
	Accessibility Permission = iota
	Monitoring
	Control
)

type State struct {
	Accessibility, Monitoring, Control bool
}

func Read(accessibility, monitoring, control func() uint8) State {
	p := State{Accessibility: accessibility() != 0}
	if p.Accessibility {
		p.Monitoring, p.Control = monitoring() != 0, control() != 0
	}
	return p
}

func (p State) Missing() (Permission, bool) {
	switch {
	case !p.Accessibility:
		return Accessibility, true
	case !p.Monitoring:
		return Monitoring, true
	case !p.Control:
		return Control, true
	default:
		return 0, false
	}
}

type Backend interface {
	Snapshot() (State, error)
	Request(Permission) error
	OpenSettings(Permission) error
}

type Controller struct {
	Backend         Backend
	Current         State
	Err             error
	Checked         bool
	HadMissing      bool
	RestartRequired bool
	Prompted        [3]bool
}

func (p *Controller) Refresh() {
	state, err := p.Backend.Snapshot()
	p.Err, p.Checked = err, true
	if err != nil {
		return
	}
	p.Current = state
	_, missing := state.Missing()
	p.HadMissing = p.HadMissing || missing
	// A frontend preflight is not proof that an existing backend recovered.
	p.RestartRequired = p.HadMissing && !missing
}

func (p *Controller) Visible() bool {
	if !p.Checked {
		return false
	}
	_, missing := p.Current.Missing()
	return p.Err != nil || missing || p.RestartRequired
}

func (p *Controller) OpenSettings() {
	p.Refresh()
	if p.Err != nil {
		return
	}
	kind, missing := p.Current.Missing()
	if !missing {
		return
	}
	// Prompt only after an explicit click, at most once per permission per run.
	// In particular, never ask CoreGraphics for access before Accessibility.
	if !p.Prompted[kind] {
		if p.Err = p.Backend.Request(kind); p.Err != nil {
			return
		}
		p.Prompted[kind] = true
	}
	p.Err = p.Backend.OpenSettings(kind)
}

func Pane(kind Permission) string {
	if kind == Monitoring {
		return "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent"
	}
	return "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
}
