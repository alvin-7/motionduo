package app

import (
	"encoding/json"
	"fmt"
	"image/png"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/alvin-7/mousehop/mygo/internal/model"
	"github.com/egoist/mygo/ui"
)

func fixture() *application {
	a := newApplication()
	a.model.DeviceName = "Studio-PC"
	a.model.Online, a.model.EmailLoaded, a.model.Email = true, true, "desk@example.com"
	a.model.Capture, a.model.Emulation = "Enabled", "Enabled"
	a.model.ReleaseBind, a.model.ReleaseBindLoaded = append([]string{}, DefaultBind...), true
	a.model.ReleaseThreshold, a.model.ThresholdLoaded = 100, true
	a.model.LocalDisplaySize = &[2]uint32{1920, 1080}
	host := "MacBook Pro"
	a.model.Clients[7] = model.Client{ID: 7, Config: model.ClientConfig{Hostname: &host, Position: "right", Port: 4252, Alignment: "top_left", Mode: "fastest", Modifier: "control", KCP: true}, State: model.ClientState{Active: true, Alive: true, Transport: "KCP", DisplaySize: &[2]uint32{1280, 800}}}
	a.model.Nearby = []model.DiscoveredDevice{{ID: "nearby-id", Name: "Studio PC", Hostname: "studio.local", Platform: "windows", Port: 4252}}
	a.model.Incoming["hidden-certificate"] = model.IncomingPeer{Description: "MacBook Pro", Sensitivity: 1}
	a.model.Pairing = []model.PairingRequest{{ID: "hidden-certificate", Name: "Office desktop", Addresses: []string{"192.168.1.24"}}}
	a.request = func(string, any) error { return nil }
	return a
}

func TestLocalDeviceNameIsShownWithoutDisplacingNavigation(t *testing.T) {
	for _, language := range []string{"en", "zh-Hans"} {
		t.Run(language, func(t *testing.T) {
			useLanguage(t, language)
			for _, width := range []int{minWindowWidth, windowWidth} {
				for _, name := range []string{"Studio-PC", "Studio-workstation-with-a-long-device-name-for-confirmation"} {
					a := fixture()
					a.model.Pairing = nil
					data, err := json.Marshal(map[string]string{"LocalDeviceName": name})
					if err != nil {
						t.Fatal(err)
					}
					if _, err := a.receiveEvent(data); err != nil {
						t.Fatal(err)
					}
					tt := ui.NewTester(a.view, width, minWindowHeight)
					if !tt.HasText(name) || !tt.HasText("MotionDuo") {
						t.Fatal("toolbar does not identify this computer", tt.Texts())
					}
					for _, label := range []string{"Devices", "Screen layout", "Settings"} {
						r := findScreen(t, tt, tr(label))
						if r.X < 0 || r.X+r.W > float32(width) {
							t.Fatal("device name displaced navigation", name, label, r)
						}
					}
					r := findScreen(t, tt, name)
					tt.Move(r.X+r.W/2, r.Y+r.H/2)
					tt.Advance(700 * time.Millisecond)
					if strings.Count(strings.Join(tt.Texts(), "\n"), name) < 2 {
						t.Fatal("hover did not reveal the complete device name", tt.Texts())
					}
					tt.Move(1, 1)
					a.page = "Screen layout"
					tt.Frame()
					if !tt.HasText(name) || !tt.HasText(tr("This computer")) {
						t.Fatal("layout does not use the same local identity", tt.Texts())
					}
				}
			}
		})
	}
}

func TestAllPagesRenderAtSupportedSizes(t *testing.T) {
	for _, page := range []string{"Devices", "Screen layout", "Settings"} {
		for _, size := range [][2]int{{windowWidth, windowHeight}, {minWindowWidth, minWindowHeight}} {
			t.Run(fmt.Sprintf("%s/%d", page, size[0]), func(t *testing.T) {
				a := fixture()
				a.page = page
				tt := ui.NewTester(a.view, size[0], size[1])
				if !tt.HasText(page) {
					t.Fatal(tt.Texts())
				}
				for _, text := range tt.Texts() {
					if strings.Contains(text, "hidden-certificate") {
						t.Fatal("certificate rendered")
					}
				}
				if dir := os.Getenv("MOUSEHOP_TEST_SNAPSHOTS"); dir != "" {
					if err := os.MkdirAll(dir, 0755); err != nil {
						t.Fatal(err)
					}
					file, err := os.Create(filepath.Join(dir, fmt.Sprintf("%s-%d.png", strings.ReplaceAll(page, " ", "-"), size[0])))
					if err != nil {
						t.Fatal(err)
					}
					err = png.Encode(file, tt.Image())
					_ = file.Close()
					if err != nil {
						t.Fatal(err)
					}
				}
			})
		}
	}
}

func TestSnapPreviewRenderFixture(t *testing.T) {
	a := fixture()
	a.page, a.model.Pairing = "Screen layout", nil
	tt := ui.NewTester(a.view, windowWidth, windowHeight)
	startLayoutDrag(t, tt, screenPlacement{"left", "top_left"})
	if !tt.HasText("Top left") || !a.layoutGesture.Magnetic {
		t.Fatal("corner preview did not engage")
	}
	if dir := os.Getenv("MOUSEHOP_TEST_SNAPSHOTS"); dir != "" {
		if err := os.MkdirAll(dir, 0755); err != nil {
			t.Fatal(err)
		}
		file, err := os.Create(filepath.Join(dir, "Screen-layout-snapping.png"))
		if err != nil {
			t.Fatal(err)
		}
		defer file.Close()
		if err := png.Encode(file, tt.Image()); err != nil {
			t.Fatal(err)
		}
	}
}

func TestCompactSettingsHideAdvancedUntilRequested(t *testing.T) {
	a := fixture()
	a.page, a.model.Pairing = "Settings", nil
	writes := 0
	a.request = func(string, any) error { writes++; return nil }
	tt := ui.NewTester(a.view, minWindowWidth, minWindowHeight)
	for _, label := range []string{"Edit shortcut", "Listen port", "Connection details", "Remove access", "Private application"} {
		if _, ok := tt.Find(label); ok {
			t.Fatalf("low-frequency control visible by default: %s", label)
		}
	}
	clickRelease(t, tt, "Advanced")
	tt.Advance(250 * time.Millisecond)
	if !a.advancedOpen || !tt.HasText("Return control") {
		t.Fatal("advanced options no longer reachable", tt.Texts())
	}
	if writes != 0 {
		t.Fatal("disclosing settings wrote configuration")
	}
}

func TestCompactCoreActionsAndCanvasFit(t *testing.T) {
	for _, size := range [][2]int{{windowWidth, windowHeight}, {minWindowWidth, minWindowHeight}} {
		a := fixture()
		tt := ui.NewTester(a.view, size[0], size[1])
		for _, label := range []string{"Allow", "Decline", "Connect", "Share with MacBook Pro"} {
			r := findScreen(t, tt, tr(label))
			if r.X < 0 || r.Y < 0 || r.X+r.W > float32(size[0]) || r.Y+r.H > float32(size[1]) {
				t.Fatalf("%s is clipped at %v: %v", label, size, r)
			}
		}
		a.page = "Screen layout"
		tt.Frame()
		stage := findScreen(t, tt, "Screen arrangement")
		if abs(stage.W-float32(size[0]-28)) > 1 || stage.Y+stage.H > float32(size[1]) {
			t.Fatalf("canvas geometry differs from compact content bounds: %v", stage)
		}
		if tt.HasText("Arrange") || tt.HasText("KCP") {
			t.Fatal("redundant or transport controls leaked into main UI")
		}
	}
}

func TestHiddenAdvancedSaveStillTimesOut(t *testing.T) {
	a := fixture()
	a.model.Pairing = nil
	tt := ui.NewTester(a.view, minWindowWidth, minWindowHeight)
	a.releasePending = &PendingShortcut{Before: append([]string{}, a.model.ReleaseBind...), Target: []string{"KeyF12"}, Until: tt.Now().Add(time.Second)}
	e := a.connectionEditor(7)
	before := peerConnection(a.model.Clients[7])
	target := before
	target.Port++
	e.Pending = &pendingConnection{before, target, tt.Now().Add(time.Second)}
	tt.Frame()
	tt.Advance(2 * time.Second)
	if a.releasePending != nil || e.Pending != nil || !tt.HasText("not confirmed; try again") || tt.FramePending() {
		t.Fatalf("hidden save pending release=%v connection=%v frame=%v texts=%v", a.releasePending, e.Pending, tt.FramePending(), tt.Texts())
	}
}

func TestConnectionRequestStaysVisibleAboveScrolledSettings(t *testing.T) {
	a := fixture()
	a.page, a.advancedOpen, a.model.Pairing = "Settings", true, nil
	tt := ui.NewTester(a.view, minWindowWidth, minWindowHeight)
	tt.Scroll(300, 300, 0, 600)
	a.model.Pairing = []model.PairingRequest{{ID: "actual-request", Name: "Office desktop"}}
	tt.Frame()
	allow := findScreen(t, tt, "Allow")
	if allow.Y < 50 || allow.Y+allow.H > 200 {
		t.Fatal("request followed the scrolled settings out of view", allow)
	}
	var sent string
	a.request = func(kind string, value any) error {
		data, _ := json.Marshal(map[string]any{kind: value})
		sent = string(data)
		return nil
	}
	clickRelease(t, tt, "Allow")
	if sent != `{"RespondPairing":["actual-request",true]}` {
		t.Fatal(sent)
	}
}

func TestAllowConnectionUsesOpaqueRequestNotDisplayedFingerprint(t *testing.T) {
	a := fixture()
	var kind string
	var value any
	a.request = func(k string, v any) error { kind, value = k, v; return nil }
	tt := ui.NewTester(a.view, 1100, 760)
	if err := tt.Click("Allow"); err != nil {
		t.Fatal(err)
	}
	encoded, _ := json.Marshal(value)
	if kind != "RespondPairing" || string(encoded) != `["hidden-certificate",true]` {
		t.Fatalf("%s %s", kind, encoded)
	}
}

func TestDeclineDoesNotAuthorizeTheRequest(t *testing.T) {
	a := fixture()
	var kind string
	var value any
	a.request = func(k string, v any) error { kind, value = k, v; return nil }
	tt := ui.NewTester(a.view, 820, 600)
	if err := tt.Click("Decline"); err != nil {
		t.Fatal(err)
	}
	encoded, _ := json.Marshal(value)
	if kind != "RespondPairing" || string(encoded) != `["hidden-certificate",false]` {
		t.Fatalf("%s %s", kind, encoded)
	}
}

func TestConnectReachesDaemonAndLayoutHasNoPlacementButtons(t *testing.T) {
	a := fixture()
	a.model.Pairing = nil
	var calls []string
	a.request = func(k string, v any) error {
		data, _ := json.Marshal(map[string]any{k: v})
		calls = append(calls, string(data))
		return nil
	}
	tt := ui.NewTester(a.view, 1100, 760)
	if err := tt.Click("Connect"); err != nil {
		t.Fatal(err)
	}
	if calls[len(calls)-1] != `{"ConnectDiscovered":["nearby-id","right"]}` {
		t.Fatal(calls)
	}
	a.page = "Screen layout"
	tt.Frame()
	for _, name := range []string{"Left", "Right", "Top", "Bottom", "Top left", "Top right", "Bottom left", "Bottom right", "Scale to fit"} {
		if tt.HasText(name) {
			t.Fatalf("layout still exposes placement button %q", name)
		}
	}
}

func TestOnboardingAndEmptyLayout(t *testing.T) {
	a := newApplication()
	a.model.Online, a.model.EmailLoaded = true, true
	var saved any
	a.request = func(kind string, value any) error {
		if kind == "SetDiscoveryEmail" {
			saved = value
		}
		return nil
	}
	tt := ui.NewTester(a.view, 1100, 760)
	if _, exists := tt.Find("Email address"); exists || tt.HasText("Discovery email") {
		t.Fatal("Devices duplicated the email form", tt.Texts())
	}
	clickRelease(t, tt, "Set discovery email")
	if a.page != "Settings" {
		t.Fatal("email setup did not navigate to Settings")
	}
	if err := tt.Click("Email address"); err != nil {
		t.Fatal(err)
	}
	tt.Type("person@example.com")
	if err := tt.Click("Save email"); err != nil {
		t.Fatal(err)
	}
	if saved != "person@example.com" {
		t.Fatalf("saved %#v", saved)
	}
	a.page = "Screen layout"
	tt.Frame()
	if !tt.HasText("Connect a computer first") {
		t.Fatal(tt.Texts())
	}
}

func TestEmailSetupIsNotDuplicatedAtCompactSizes(t *testing.T) {
	for _, width := range []int{windowWidth, minWindowWidth} {
		t.Run(fmt.Sprint(width), func(t *testing.T) {
			a := fixture()
			a.model.Email, a.model.Pairing, a.model.Nearby = "", nil, nil
			writes := 0
			a.request = func(string, any) error { writes++; return nil }
			tt := ui.NewTester(a.view, width, minWindowHeight)
			if _, exists := tt.Find("Email address"); exists || tt.HasText("Save email") {
				t.Fatal("Devices contains an email form")
			}
			clickRelease(t, tt, "Set discovery email")
			count := 0
			for _, text := range tt.Texts() {
				if text == "Discovery email" {
					count++
				}
			}
			if a.page != "Settings" || count != 1 || writes != 0 {
				t.Fatal("setup must open the single Settings form without saving", count, writes, tt.Texts())
			}
		})
	}
}
