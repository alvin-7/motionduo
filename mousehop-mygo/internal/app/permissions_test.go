package app

import (
	"errors"
	"image/png"
	"os"
	"path/filepath"
	"slices"
	"testing"

	"github.com/egoist/mygo/ui"
)

type fakePermissions struct {
	state                    State
	err, requestErr, openErr error
	requests, opened         []Permission
}

func (f *fakePermissions) Snapshot() (State, error) { return f.state, f.err }
func (f *fakePermissions) Request(kind Permission) error {
	f.requests = append(f.requests, kind)
	return f.requestErr
}
func (f *fakePermissions) OpenSettings(kind Permission) error {
	f.opened = append(f.opened, kind)
	return f.openErr
}

func TestMacPermissionPreflightDoesNotTouchCoreGraphicsWithoutAccessibility(t *testing.T) {
	for _, trusted := range []uint8{0, 1, 255} {
		graphicsChecks := 0
		graphics := func() uint8 { graphicsChecks++; return 1 }
		state := Read(func() uint8 { return trusted }, graphics, graphics)
		if trusted == 0 {
			if graphicsChecks != 0 || state != (State{}) {
				t.Fatal("CoreGraphics queried before Accessibility", state)
			}
		} else if graphicsChecks != 2 || state != (State{true, true, true}) {
			t.Fatal("native Boolean was not normalized", trusted, state)
		}
	}
}

func TestPermissionPromptsAreExplicitSequentialAndNotRepeated(t *testing.T) {
	f := &fakePermissions{}
	p := &Controller{Backend: f}
	if p.Visible() {
		t.Fatal("uninitialized permissions displayed as denied")
	}
	p.Refresh()
	p.Refresh()
	if !p.Visible() || len(f.requests) != 0 || len(f.opened) != 0 {
		t.Fatal("preflight prompted or opened system settings")
	}
	p.OpenSettings()
	p.OpenSettings()
	if !slices.Equal(f.requests, []Permission{Accessibility}) || len(f.opened) != 2 {
		t.Fatal("repeated click queued duplicate native prompts", f.requests)
	}
	f.state.Accessibility = true
	p.OpenSettings()
	f.state.Monitoring = true
	p.OpenSettings()
	if !slices.Equal(f.requests, []Permission{Accessibility, Monitoring, Control}) {
		t.Fatal("permissions were requested out of order", f.requests)
	}
	if p.RestartRequired || !p.Visible() {
		t.Fatal("request return was treated as approval")
	}
	f.state.Control = true
	p.Refresh()
	if !p.RestartRequired || !p.Visible() {
		t.Fatal("new permission grant lost backend restart requirement")
	}
	f.state.Accessibility = false
	p.Refresh()
	if p.RestartRequired {
		t.Fatal("revoked access still presented as ready to restart")
	}
	fresh := &Controller{Backend: &fakePermissions{state: State{true, true, true}}}
	fresh.Refresh()
	if fresh.Visible() {
		t.Fatal("healthy startup adds an unnecessary permission card")
	}
}

func TestPermissionFailuresAreNotTreatedAsDeniedOrGranted(t *testing.T) {
	f := &fakePermissions{err: errors.New("API unavailable")}
	p := &Controller{Backend: f}
	p.OpenSettings()
	if !p.Visible() || p.Err == nil || len(f.requests) != 0 {
		t.Fatal("snapshot error prompted for an unknown permission")
	}
	f.err, f.requestErr = nil, errors.New("request failed")
	p.OpenSettings()
	if p.Err == nil || p.Prompted[Accessibility] || len(f.opened) != 0 {
		t.Fatal("request failure marked permission prompted or opened settings")
	}
	f.requestErr, f.openErr = nil, errors.New("settings unavailable")
	p.OpenSettings()
	if p.Err == nil || !p.Prompted[Accessibility] || p.RestartRequired {
		t.Fatal("failed settings navigation pretended access was granted")
	}
	if Pane(Monitoring) == Pane(Accessibility) || Pane(Control) != Pane(Accessibility) {
		t.Fatal("wrong macOS permission pane")
	}
}

func TestMacOnboardingIsCompactAndDoesNotAuthorizeDevices(t *testing.T) {
	a := fixture()
	a.model.Pairing = nil
	a.model.Capture, a.model.Emulation = "Disabled", "Disabled"
	f := &fakePermissions{}
	a.permissions = &Controller{Backend: f}
	a.permissions.Refresh()
	requests, restarts := 0, 0
	a.request = func(string, any) error { requests++; return nil }
	a.restart = func() { restarts++ }
	tt := ui.NewTester(a.view, minWindowWidth, minWindowHeight)
	if _, ok := tt.Find("Input access needed"); !ok {
		t.Fatal("missing permission was presented as Ready")
	}
	for _, label := range []string{"Open settings", "Check again"} {
		r := findScreen(t, tt, label)
		if r.X < 0 || r.X+r.W > minWindowWidth || r.Y+r.H > minWindowHeight {
			t.Fatal("permission action clipped", label, r)
		}
	}
	if dir := os.Getenv("MOUSEHOP_TEST_SNAPSHOTS"); dir != "" {
		if err := os.MkdirAll(dir, 0755); err != nil {
			t.Fatal(err)
		}
		file, err := os.Create(filepath.Join(dir, "Mac-input-access-600.png"))
		if err != nil {
			t.Fatal(err)
		}
		defer file.Close()
		if err := png.Encode(file, tt.Image()); err != nil {
			t.Fatal(err)
		}
	}
	clickRelease(t, tt, "Open settings")
	if len(f.requests) != 1 || requests != 0 || restarts != 0 {
		t.Fatal("permission action changed device trust or restarted automatically")
	}
	f.state = State{true, true, true}
	clickRelease(t, tt, "Check again")
	if !tt.HasText("Restart MotionDuo") || a.model.Capture != "Disabled" || a.model.Emulation != "Disabled" {
		t.Fatal("UI preflight was confused with backend recovery", tt.Texts())
	}
	clickRelease(t, tt, "Restart MotionDuo")
	if restarts != 1 || requests != 0 {
		t.Fatal("explicit restart did not use the resident lifecycle")
	}
}
