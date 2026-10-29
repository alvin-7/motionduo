package app

import (
	"encoding/json"
	"fmt"
	"image/png"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/egoist/mygo/ui"
)

func clickDeviceConfig(t *testing.T, tt *ui.Tester, label string) {
	t.Helper()
	clickRelease(t, tt, label)
	tt.Advance(250 * time.Millisecond)
}

func TestDeviceConfigurationExpandsBelowItsRowWithoutSaving(t *testing.T) {
	for _, width := range []int{windowWidth, minWindowWidth} {
		t.Run(fmt.Sprint(width), func(t *testing.T) {
			a := fixture()
			a.model.Pairing = nil
			writes := 0
			a.request = func(string, any) error { writes++; return nil }
			tt := ui.NewTester(a.view, width, minWindowHeight)
			for _, label := range []string{"Send clipboard", "Reliable input (KCP)", "Connection details", "Remove computer", "Receive clipboard"} {
				if _, exists := tt.Find(label); exists {
					t.Fatalf("device configuration visible before disclosure: %s", label)
				}
			}
			button := findScreen(t, tt, "Configure MacBook Pro")
			if button.X+button.W > float32(width) || button.Y+button.H > minWindowHeight {
				t.Fatal("configuration button clipped", button)
			}
			clickDeviceConfig(t, tt, "Configure MacBook Pro")
			if a.configuredDevice != clientDeviceKey(7) || !tt.HasText("Trackpad momentum") || !tt.HasText("Connection details") {
				t.Fatal("configuration did not expand on Devices", tt.Texts())
			}
			if tt.HasText("Reliable input (KCP)") || tt.HasText("Clipboard sync") {
				t.Fatal("global settings appeared in device configuration", tt.Texts())
			}
			settings := findScreen(t, tt, "Trackpad momentum")
			if settings.Y < button.Y+button.H || settings.X+settings.W > float32(width) {
				t.Fatal("configuration is not below its device row", button, settings)
			}
			if dir := os.Getenv("MOUSEHOP_TEST_SNAPSHOTS"); dir != "" {
				if err := os.MkdirAll(dir, 0755); err != nil {
					t.Fatal(err)
				}
				file, err := os.Create(filepath.Join(dir, fmt.Sprintf("Devices-configured-%d.png", width)))
				if err != nil {
					t.Fatal(err)
				}
				err = png.Encode(file, tt.Image())
				_ = file.Close()
				if err != nil {
					t.Fatal(err)
				}
			}
			clickDeviceConfig(t, tt, "Configure MacBook Pro")
			if a.configuredDevice != "" || tt.HasText("Reliable input (KCP)") || writes != 0 {
				t.Fatal("disclosure did not collapse, or wrote configuration", writes, tt.Texts())
			}
		})
	}
}

func TestSettingsAdvancedContainsOnlyApplicationSettings(t *testing.T) {
	a := fixture()
	a.page, a.advancedOpen, a.model.Pairing = "Settings", true, nil
	tt := ui.NewTester(a.view, 680, 1400)
	for _, label := range []string{"Send clipboard", "Connection details", "Remove computer", "Receive clipboard", "Natural scrolling", "Pointer sensitivity", "Remove access"} {
		if _, exists := tt.Find(label); exists || tt.HasText(label) {
			t.Fatalf("device setting remains in Settings > Advanced: %s", label)
		}
	}
	for _, label := range []string{"Return control", "Network", "Clipboard privacy", "Reliable input (KCP)", "Clipboard sync"} {
		if !tt.HasText(label) {
			t.Fatalf("global setting was removed: %s", label)
		}
	}
}

func TestDeviceConfigurationTargetsSelectedPeerAndKeepsOnlyOneOpen(t *testing.T) {
	a := fixture()
	a.model.Pairing, a.model.Incoming = nil, nil
	second := a.model.Clients[7]
	name := "Office PC"
	second.ID, second.Config.Hostname = 8, &name
	a.model.Clients[8] = second
	var calls []string
	a.request = func(kind string, value any) error {
		data, _ := json.Marshal(map[string]any{kind: value})
		calls = append(calls, string(data))
		return nil
	}
	tt := ui.NewTester(a.view, 680, 1000)
	clickDeviceConfig(t, tt, "Configure MacBook Pro")
	e := a.connectionEditor(7)
	e.Editing = true
	clickDeviceConfig(t, tt, "Configure Office PC")
	if a.configuredDevice != clientDeviceKey(8) || e.Editing {
		t.Fatal("switching devices did not close the previous draft")
	}
	row := findScreen(t, tt, "Trackpad momentum")
	// The text and switch have the same accessible name; hit the switch itself.
	tt.ClickAt(620, row.Y+row.H/2)
	if len(calls) != 1 || calls[0] != `{"SetClientScrollInertia":[8,true]}` {
		t.Fatal("configuration targeted the wrong device", calls)
	}
}

func TestIncomingConfigurationReachableWithOutgoingPeers(t *testing.T) {
	a := fixture()
	a.model.Pairing = nil
	var calls []string
	a.request = func(kind string, value any) error {
		data, _ := json.Marshal(map[string]any{kind: value})
		calls = append(calls, string(data))
		return nil
	}
	tt := ui.NewTester(a.view, 680, 1000)
	clickDeviceConfig(t, tt, "Configure incoming MacBook Pro")
	if a.configuredDevice != incomingDeviceKey("hidden-certificate") || !tt.HasText("Pointer sensitivity") || !tt.HasText("Remove access") || tt.HasText("Reliable input (KCP)") {
		t.Fatal("incoming configuration did not expand in its own device", tt.Texts())
	}
	if tt.HasText("Clipboard sync") {
		t.Fatal("clipboard switch appeared in incoming device configuration", tt.Texts())
	}
	row := findScreen(t, tt, "Natural scrolling")
	tt.ClickAt(620, row.Y+row.H/2)
	if len(calls) != 1 || calls[0] != `{"SetIncomingPeerNaturalScroll":["hidden-certificate",true]}` {
		t.Fatal("incoming configuration targeted the wrong identity", calls)
	}
}

func TestGlobalAdvancedSwitches(t *testing.T) {
	for _, language := range []string{"en", "zh-Hans"} {
		t.Run(language, func(t *testing.T) {
			useLanguage(t, language)
			a := fixture()
			a.page, a.advancedOpen, a.model.Pairing = "Settings", true, nil
			a.model.GlobalKCP, a.model.ClipboardSync = true, true
			var calls []string
			a.request = func(kind string, value any) error {
				data, _ := json.Marshal(map[string]any{kind: value})
				calls = append(calls, string(data))
				return nil
			}
			tt := ui.NewTester(a.view, 680, 1400)
			for _, item := range []struct{ label, want string }{
				{"Reliable input (KCP)", `{"SetGlobalKcp":false}`},
				{"Clipboard sync", `{"SetClipboardSync":false}`},
			} {
				row := findScreen(t, tt, tr(item.label))
				tt.ClickAt(620, row.Y+row.H/2)
				if len(calls) == 0 || calls[len(calls)-1] != item.want {
					t.Fatal(item.label, calls)
				}
			}
		})
	}
}

func TestDeviceCollapseAndPageChangeKeepPendingConfirmation(t *testing.T) {
	a := fixture()
	a.model.Pairing = nil
	a.configuredDevice = clientDeviceKey(7)
	tt := ui.NewTester(a.view, 680, 600)
	e := a.connectionEditor(7)
	before := peerConnection(a.model.Clients[7])
	target := before
	target.Port++
	e.Editing, e.Pending = true, &pendingConnection{before, target, tt.Now().Add(time.Second)}
	clickDeviceConfig(t, tt, "Configure MacBook Pro")
	if e.Editing || e.Pending == nil {
		t.Fatal("collapsing must close drafts, not a pending save")
	}
	a.page = "Settings"
	tt.Frame()
	tt.Advance(2 * time.Second)
	if e.Pending != nil || !tt.HasText("not confirmed; try again") {
		t.Fatal("collapsed device lost confirmation timeout", tt.Texts())
	}
}

func TestDeviceConnectionDraftDoesNotDependOnSettingsAdvanced(t *testing.T) {
	a := fixture()
	a.model.Pairing, a.model.Incoming = nil, nil
	a.configuredDevice = clientDeviceKey(7)
	tt := ui.NewTester(a.view, 680, 1000)
	clickRelease(t, tt, "Connection details")
	tt.Advance(250 * time.Millisecond)
	clickRelease(t, tt, "Edit connection")
	e := a.connectionEditor(7)
	tt.Frame()
	if a.advancedOpen || !e.Editing || !tt.HasText("Save connection") {
		t.Fatal("device editor still depends on Settings > Advanced", tt.Texts())
	}
	a.page = "Settings"
	tt.Frame()
	if e.Editing {
		t.Fatal("leaving Devices retained an editable connection draft")
	}
}

func TestDeviceDisclosureDoesNotSurviveDeletedIdentityOrHandleReuse(t *testing.T) {
	a := fixture()
	a.configuredDevice = clientDeviceKey(7)
	if _, err := a.receiveEvent([]byte(`{"Deleted":7}`)); err != nil {
		t.Fatal(err)
	}
	if a.configuredDevice != "" {
		t.Fatal("deleted client retained its disclosure")
	}
	peer := fixture().model.Clients[7]
	data, _ := json.Marshal(map[string]any{"Created": []any{peer.ID, peer.Config, peer.State}})
	if _, err := a.receiveEvent(data); err != nil || a.configuredDevice != "" {
		t.Fatal("reused handle inherited disclosure", err)
	}
	a.configuredDevice = incomingDeviceKey("hidden-certificate")
	if _, err := a.receiveEvent([]byte(`{"AuthorizedUpdated":{}}`)); err != nil || a.configuredDevice != "" {
		t.Fatal("removed incoming identity retained disclosure", err)
	}
}
