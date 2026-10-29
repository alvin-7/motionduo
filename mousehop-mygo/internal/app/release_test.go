package app

import (
	"encoding/json"
	"errors"
	"fmt"
	"image/png"
	"math"
	"os"
	"path/filepath"
	"regexp"
	"slices"
	"strings"
	"testing"
	"time"

	"github.com/alvin-7/mousehop/mygo/internal/model"
	"github.com/egoist/mygo/ui"
)

func TestReleasePickerRoundTripsSidesAndExistingChords(t *testing.T) {
	for _, keys := range [][]string{
		DefaultBind,
		{"KeyRightCtrl", "KeyRightShift", "KeyRightalt", "KeyRightmeta", "KeyF12"},
		{"KeyLeftCtrl", "KeyRightCtrl", "KeyLeftShift", "KeyRightShift", "KeyEsc"},
		{"KeyLeftMeta"}, {"KeyF24"}, {"KeyKpEnter"}, {"KeyLeftCtrl", "KeyA", "KeyB"},
	} {
		e := NewShortcutEditor(keys)
		if !SameBind(e.Keys(), keys) {
			t.Fatalf("%v became %v", keys, e.Keys())
		}
		if !slices.Contains(e.Options(), e.Trigger) {
			t.Fatalf("current trigger %q missing", e.Trigger)
		}
	}
	e := NewShortcutEditor([]string{"KeyRightCtrl", "KeyA", "KeyB"})
	e.Trigger = "F12"
	if !SameBind(e.Keys(), []string{"KeyRightCtrl", "KeyF12"}) {
		t.Fatal("explicit trigger replacement retained old keys", e.Keys())
	}
}

func TestReleasePickerCodesExistInSharedScancodeEnum(t *testing.T) {
	data, err := os.ReadFile("../../../input-event/src/scancode.rs")
	if err != nil {
		t.Fatal(err)
	}
	linux := strings.SplitN(string(data), "pub enum Linux {", 2)[1]
	linux = strings.SplitN(linux, "\n}", 2)[0]
	seen := map[string]bool{}
	for _, key := range append(slices.Clone(Modifiers), Triggers...) {
		if seen[key.Code] {
			t.Fatalf("duplicate key %s", key.Code)
		}
		seen[key.Code] = true
		if !regexp.MustCompile(`\b` + key.Code + `\s*=`).MatchString(linux) {
			t.Fatalf("unknown Rust enum variant %s", key.Code)
		}
	}
}

func TestReleaseLabelsPreservePhysicalSides(t *testing.T) {
	for _, platform := range []string{"windows", "darwin", "linux"} {
		for i := 0; i < len(Modifiers); i += 2 {
			left := KeyLabel(Modifiers[i].Code, platform)
			right := KeyLabel(Modifiers[i+1].Code, platform)
			if !strings.HasPrefix(left, "Left ") || !strings.HasPrefix(right, "Right ") || left == right {
				t.Fatalf("%s: %s / %s", platform, left, right)
			}
		}
	}
	if KeyLabel("KeyRightmeta", "darwin") != "Right Command" || KeyLabel("KeyRightalt", "darwin") != "Right Option" || KeyLabel("KeyLeftMeta", "windows") != "Left Win" {
		t.Fatal("platform modifier names")
	}
}

func releaseTester(a *application, width, height int) *ui.Tester {
	return ui.NewTester(func(c *ui.Context) {
		theme := graphiteTheme()
		c.SetTheme(&theme)
		ui.Column(c).Padding(20).Gap(16).Children(func() { a.releaseView(c) })
	}, width, height)
}

func clickRelease(t *testing.T, tt *ui.Tester, label string) {
	t.Helper()
	if err := tt.Click(label); err != nil {
		t.Fatal(err)
	}
}

func TestReleaseShortcutPointerEditingAndCanonicalAcknowledgment(t *testing.T) {
	a := fixture()
	a.model.ReleaseBind = []string{"KeyLeftCtrl", "KeyF12"}
	var calls []string
	a.request = func(kind string, value any) error {
		data, _ := json.Marshal(map[string]any{kind: value})
		calls = append(calls, string(data))
		return nil
	}
	tt := releaseTester(a, 580, 760)
	clickRelease(t, tt, "Edit shortcut")
	clickRelease(t, tt, "Left Ctrl modifier")
	clickRelease(t, tt, "Right Ctrl modifier")
	clickRelease(t, tt, "Right Shift modifier")
	clickRelease(t, tt, "Trigger key")
	tt.Key(ui.Cmd, ui.KeyA)
	tt.Type("F11")
	tt.Key(0, ui.KeyEnter)
	if len(calls) != 0 {
		t.Fatal("draft edits wrote to daemon", calls)
	}
	clickRelease(t, tt, "Apply shortcut")
	if len(calls) != 1 || calls[0] != `{"SetReleaseBind":["KeyRightCtrl","KeyRightShift","KeyF11"]}` {
		t.Fatal(calls)
	}
	if a.releasePending == nil || !SameBind(a.model.ReleaseBind, []string{"KeyLeftCtrl", "KeyF12"}) || !tt.HasText("Saving...") {
		t.Fatal("queued request was shown as confirmed")
	}
	if err := a.model.Apply([]byte(`{"ReleaseBind":["KeyRightCtrl","KeyRightShift","KeyF11"]}`)); err != nil {
		t.Fatal(err)
	}
	tt.Frame()
	if a.releasePending != nil || !tt.HasText("Right Ctrl + Right Shift + F11") {
		t.Fatal("canonical response not shown", tt.Texts())
	}
}

func TestReleaseShortcutCancelResetEmptyAndSendFailure(t *testing.T) {
	a := fixture()
	a.model.ReleaseBind = []string{"KeyRightCtrl"}
	var calls [][]string
	a.request = func(kind string, value any) error {
		if kind != "SetReleaseBind" {
			t.Fatal(kind)
		}
		calls = append(calls, value.([]string))
		return nil
	}
	tt := releaseTester(a, 580, 760)
	clickRelease(t, tt, "Edit shortcut")
	clickRelease(t, tt, "Right Ctrl modifier")
	clickRelease(t, tt, "Apply shortcut")
	if len(calls) != 0 {
		t.Fatal("empty shortcut sent; empty means reset, not disabled")
	}
	clickRelease(t, tt, "Cancel shortcut")
	if a.releaseEditor.Open || len(calls) != 0 {
		t.Fatal("cancel wrote a shortcut")
	}
	clickRelease(t, tt, "Edit shortcut")
	clickRelease(t, tt, "Use default")
	if len(calls) != 1 || calls[0] == nil || len(calls[0]) != 0 {
		t.Fatal("default must encode [] rather than null", calls)
	}
	if err := a.model.Apply([]byte(`{"ReleaseBind":["KeyLeftCtrl","KeyLeftShift","KeyLeftMeta","KeyLeftAlt"]}`)); err != nil {
		t.Fatal(err)
	}
	tt.Frame()
	if a.releasePending != nil {
		t.Fatal("default echo not confirmed")
	}
	a.request = func(string, any) error { return errors.New("queue is full") }
	clickRelease(t, tt, "Edit shortcut")
	clickRelease(t, tt, "Right Ctrl modifier")
	clickRelease(t, tt, "Apply shortcut")
	if !a.releaseEditor.Open || a.releasePending != nil || a.model.Error != "queue is full" {
		t.Fatal("failed send discarded draft or pretended to save")
	}
}

func TestReleaseShortcutTimeoutAndNoWriteBeforeSettingsLoaded(t *testing.T) {
	a := fixture()
	sent := false
	a.request = func(string, any) error { sent = true; return nil }
	a.model.ReleaseBindLoaded, a.model.ThresholdLoaded = false, false
	tt := releaseTester(a, 580, 760)
	clickRelease(t, tt, "Edit shortcut")
	clickRelease(t, tt, "Apply threshold")
	if sent || a.releaseEditor.Open {
		t.Fatal("settings mutated before Sync events")
	}
	a.model.ReleaseBindLoaded, a.model.ThresholdLoaded = true, true
	tt.Frame()
	clickRelease(t, tt, "Edit shortcut")
	clickRelease(t, tt, "Right Ctrl modifier")
	clickRelease(t, tt, "Apply shortcut")
	if !sent || a.releasePending == nil {
		t.Fatal("shortcut was not queued")
	}
	a.releasePending.Until = time.Time{}
	tt.Frame()
	if a.releasePending != nil || a.model.Error != "Shortcut was not confirmed; try again" {
		t.Fatal("unconfirmed shortcut did not time out")
	}
}

func TestReleaseThresholdValidatedAndPreservesLoadedRange(t *testing.T) {
	for _, value := range []float64{-1, 0.1, math.NaN(), math.Inf(1), math.Inf(-1), math.MaxUint32 + 1} {
		if _, ok := ThresholdValue(value); ok {
			t.Fatalf("accepted %v", value)
		}
	}
	for _, value := range []uint32{0, 50, 500, 1500, math.MaxUint32} {
		if got, ok := ThresholdValue(float64(value)); !ok || got != value {
			t.Fatal(value, got, ok)
		}
	}
	now := time.Now()
	e := ThresholdEditor{}
	e.Sync(1500, now)
	if e.Value != 1500 {
		t.Fatal("existing value clamped to old slider limit")
	}
	e.Value = 200
	e.Sync(1000, now)
	if e.Value != 200 || e.Confirmed != 1000 {
		t.Fatal("external update discarded unsaved draft")
	}
	e.Pending = &PendingThreshold{200, now.Add(time.Second)}
	e.Sync(1000, now)
	if e.Pending == nil {
		t.Fatal("old state falsely acknowledged save")
	}
	e.Sync(200, now)
	if e.Pending != nil || e.Value != 200 || e.Confirmed != 200 {
		t.Fatal("acknowledgment not applied")
	}
	e.Value, e.Pending = 0, &PendingThreshold{0, now}
	if e.Sync(200, now) || e.Pending != nil || e.Value != 200 {
		t.Fatal("timeout did not revert to canonical value")
	}
}

func TestReleaseThresholdPointerEditDisableAndFailure(t *testing.T) {
	a := fixture()
	var calls []uint32
	a.request = func(kind string, value any) error {
		if kind != "SetReleaseThreshold" {
			t.Fatal(kind)
		}
		calls = append(calls, value.(uint32))
		return nil
	}
	tt := releaseTester(a, 580, 760)
	field, ok := tt.Find("Wall-press fallback pixels")
	if !ok {
		t.Fatal("threshold field missing")
	}
	// NumberInput's labeled row includes two stepper buttons; click the text field.
	tt.ClickAt(field.X+20, field.Y+field.H/2)
	tt.Key(ui.Cmd, ui.KeyA)
	tt.Type("0")
	if len(calls) != 0 {
		t.Fatal("draft threshold wrote without apply")
	}
	clickRelease(t, tt, "Apply threshold")
	if len(calls) != 1 || calls[0] != 0 || a.thresholdEditor.Pending == nil || a.model.ReleaseThreshold != 100 {
		t.Fatal("disabled value not queued correctly", calls)
	}
	if err := a.model.Apply([]byte(`{"ReleaseThreshold":0}`)); err != nil {
		t.Fatal(err)
	}
	tt.Frame()
	if a.thresholdEditor.Pending != nil || a.thresholdEditor.Value != 0 {
		t.Fatal("threshold echo not applied")
	}
	a.request = func(string, any) error { return errors.New("queue is full") }
	a.thresholdEditor.Value = 200
	tt.Frame()
	clickRelease(t, tt, "Apply threshold")
	if a.thresholdEditor.Value != 200 || a.thresholdEditor.Pending != nil || a.model.Error != "queue is full" {
		t.Fatal("failed threshold send lost draft or pretended to save")
	}
}

func TestReleaseSettingsEventsAndReconnectReadiness(t *testing.T) {
	m := model.NewModel()
	if err := m.Apply([]byte(`{"ReleaseBind":["KeyRightShift","KeyF12"]}`)); err != nil {
		t.Fatal(err)
	}
	if err := m.Apply([]byte(`{"ReleaseThreshold":4294967295}`)); err != nil {
		t.Fatal(err)
	}
	if !m.ReleaseBindLoaded || !m.ThresholdLoaded || m.ReleaseThreshold != math.MaxUint32 {
		t.Fatal("settings not ready after canonical events")
	}
	m.Disconnect()
	if m.ReleaseBindLoaded || m.ThresholdLoaded {
		t.Fatal("reconnect reused old settings readiness")
	}
	for _, bad := range []string{`{"ReleaseBind":1}`, `{"ReleaseThreshold":-1}`} {
		if err := m.Apply([]byte(bad)); err == nil || m.ReleaseBindLoaded || m.ThresholdLoaded {
			t.Fatal("invalid settings event became ready", bad)
		}
	}
}

func TestReleaseEditsCancelOnPageChangeAndDisconnect(t *testing.T) {
	a := fixture()
	a.page, a.model.Pairing = "Settings", nil
	a.releaseEditor = NewShortcutEditor(a.model.ReleaseBind)
	tt := ui.NewTester(a.view, 1100, 760)
	a.page = "Devices"
	tt.Frame()
	if a.releaseEditor.Open {
		t.Fatal("hidden page kept an open shortcut draft")
	}
	a.page = "Settings"
	a.releaseEditor = NewShortcutEditor(a.model.ReleaseBind)
	a.releasePending = &PendingShortcut{Target: []string{"KeyF12"}, Until: time.Now().Add(time.Second)}
	a.thresholdEditor.Pending = &PendingThreshold{0, time.Now().Add(time.Second)}
	a.model.Disconnect()
	tt.Frame()
	if a.releaseEditor.Open || a.releasePending != nil || a.thresholdEditor.Pending != nil || a.thresholdEditor.Initialized {
		t.Fatal("disconnect retained pending edits")
	}
}

func TestReleaseThresholdTimeoutRevertsAndOfflineBlocksWrites(t *testing.T) {
	a := fixture()
	tt := releaseTester(a, 580, 760)
	a.thresholdEditor.Value = 0
	a.thresholdEditor.Pending = &PendingThreshold{0, time.Time{}}
	tt.Frame()
	if a.thresholdEditor.Value != 100 || a.thresholdEditor.Pending != nil || a.model.Error != "Release threshold was not confirmed; try again" {
		t.Fatal("threshold timeout showed unconfirmed value")
	}
	a.model.Online = false
	a.thresholdEditor.Value = 200
	sent := false
	a.request = func(string, any) error { sent = true; return nil }
	tt.Frame()
	clickRelease(t, tt, "Edit shortcut")
	clickRelease(t, tt, "Apply threshold")
	if sent || a.releaseEditor.Open {
		t.Fatal("offline controls mutated daemon")
	}
}

func TestReleaseTimeoutBannerAppearsWithoutUserInput(t *testing.T) {
	for _, setting := range []string{"shortcut", "threshold"} {
		t.Run(setting, func(t *testing.T) {
			a := fixture()
			a.page, a.model.Pairing = "Settings", nil
			tt := ui.NewTester(a.view, 1100, 760)
			if setting == "shortcut" {
				a.releasePending = &PendingShortcut{Before: slices.Clone(a.model.ReleaseBind), Target: []string{"KeyF12"}, Until: tt.Now().Add(time.Second)}
			} else {
				a.thresholdEditor.Value = 0
				a.thresholdEditor.Pending = &PendingThreshold{0, tt.Now().Add(time.Second)}
			}
			tt.Frame()
			tt.Advance(2 * time.Second)
			if !tt.HasText("was not confirmed; try again") || tt.FramePending() {
				t.Fatal("timeout feedback requires another user event or keeps polling", tt.Texts())
			}
		})
	}
}

func TestReleaseEditorFixtureAtSupportedWidths(t *testing.T) {
	for _, width := range []int{540, 820} {
		t.Run(fmt.Sprint(width), func(t *testing.T) {
			a := fixture()
			a.releaseEditor = NewShortcutEditor(a.model.ReleaseBind)
			tt := releaseTester(a, width, 700)
			for _, label := range []string{"Edit shortcut", "Right Ctrl", "Trigger key", "Apply shortcut", "Apply threshold"} {
				r, ok := tt.Find(label)
				if !ok || r.X < 0 || r.X+r.W > float32(width)+0.1 || r.Y+r.H > 700 {
					t.Fatalf("%s out of bounds: %+v", label, r)
				}
			}
			stepper, _ := tt.Find("Increase")
			apply, _ := tt.Find("Apply threshold")
			if stepper.X+stepper.W >= apply.X {
				t.Fatal("numeric stepper overlaps Apply", stepper, apply)
			}
			if dir := os.Getenv("MOUSEHOP_TEST_SNAPSHOTS"); dir != "" {
				if err := os.MkdirAll(dir, 0755); err != nil {
					t.Fatal(err)
				}
				file, err := os.Create(filepath.Join(dir, fmt.Sprintf("Release-settings-%d.png", width)))
				if err != nil {
					t.Fatal(err)
				}
				defer file.Close()
				if err := png.Encode(file, tt.Image()); err != nil {
					t.Fatal(err)
				}
			}
		})
	}
}
