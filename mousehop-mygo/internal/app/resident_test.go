package app

import (
	"errors"
	"reflect"
	"testing"

	"github.com/egoist/mygo"
)

type fakeResidentWindow struct {
	minimized bool
	calls     []string
}

func (w *fakeResidentWindow) Show()             { w.calls = append(w.calls, "show") }
func (w *fakeResidentWindow) Hide()             { w.calls = append(w.calls, "hide") }
func (w *fakeResidentWindow) Focus()            { w.calls = append(w.calls, "focus") }
func (w *fakeResidentWindow) IsMinimized() bool { return w.minimized }
func (w *fakeResidentWindow) Restore()          { w.calls = append(w.calls, "restore"); w.minimized = false }

func TestResidentWindowHidesInsteadOfMinimizing(t *testing.T) {
	w := &fakeResidentWindow{}
	parkResidentWindow(w)
	openResidentWindow(w)
	want := []string{"hide", "show", "focus"}
	if !reflect.DeepEqual(w.calls, want) {
		t.Fatalf("calls %v, want %v", w.calls, want)
	}
}

func TestOpenResidentWindowRestoresManualMinimize(t *testing.T) {
	w := &fakeResidentWindow{minimized: true}
	openResidentWindow(w)
	want := []string{"restore", "show", "focus"}
	if !reflect.DeepEqual(w.calls, want) {
		t.Fatalf("calls %v, want %v", w.calls, want)
	}
}

func TestMacResidentHasNoDockIcon(t *testing.T) {
	if residentActivationPolicy("darwin") != mygo.ActivationPolicyAccessory {
		t.Fatal("macOS resident app must use only its menu-bar icon, not the Dock")
	}
	if residentActivationPolicy("windows") != mygo.ActivationPolicyRegular {
		t.Fatal("macOS activation policy must not affect the Windows window")
	}
}

type fakeLoginSettings struct {
	enabled bool
	err     error
	writes  int
}

func (s *fakeLoginSettings) OpenAtLogin() bool { return s.enabled }
func (s *fakeLoginSettings) SetOpenAtLogin(v bool) error {
	s.writes++
	if s.err == nil {
		s.enabled = v
	}
	return s.err
}

func TestResidentMenuDoesNotEnableLoginWithoutUserAction(t *testing.T) {
	login := &fakeLoginSettings{}
	opened, restarted := false, false
	m := newResidentMenu(login, func() { opened = true }, func() { restarted = true }, func(err error) { t.Fatal(err) })
	if login.writes != 0 || m.menu.ItemByID("open-at-login").Checked {
		t.Fatal("opening the app must not register login startup")
	}
	m.menu.ItemByID("open").Click(nil, nil)
	m.menu.ItemByID("restart").Click(nil, nil)
	if !opened || !restarted || m.menu.ItemByID("quit").Role != mygo.RoleQuit {
		t.Fatal("resident actions missing")
	}
	item := m.menu.ItemByID("open-at-login")
	item.Checked = true // MyGo toggles checkboxes before their Click callback.
	item.Click(item, nil)
	if !login.enabled || !item.Checked || login.writes != 1 {
		t.Fatal("login preference was not saved")
	}
	login.enabled = false // e.g. disabled by the user in system settings.
	m.refresh()
	if item.Checked || login.writes != 1 {
		t.Fatal("refresh must use OS state without changing it")
	}
}

func TestResidentMenuRollsBackLoginCheckOnSystemError(t *testing.T) {
	for _, enabled := range []bool{false, true} {
		login := &fakeLoginSettings{enabled: enabled, err: errors.New("system approval required")}
		var reported error
		m := newResidentMenu(login, func() {}, func() {}, func(err error) { reported = err })
		item := m.menu.ItemByID("open-at-login")
		item.Checked = !enabled
		item.Click(item, nil)
		if reported != login.err || item.Checked != enabled || login.enabled != enabled {
			t.Fatal("failed login change must show the error and retain actual OS state")
		}
	}
}
