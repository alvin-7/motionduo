package app

import (
	"testing"

	"github.com/egoist/mygo/ui"
)

func TestFullDiskHintConditions(t *testing.T) {
	tests := []struct {
		name, want                        string
		clipboardSync, granted, supported bool
	}{
		{name: "clipboard sync enabled without access", want: "show", clipboardSync: true, granted: false, supported: true},
		{name: "clipboard sync disabled", want: "hide", clipboardSync: false, granted: false, supported: true},
		{name: "access granted", want: "hide", clipboardSync: true, granted: true, supported: true},
		{name: "non macOS", want: "hide", clipboardSync: true, granted: false, supported: false},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			got := shouldShowFullDiskHint(test.clipboardSync, test.granted, test.supported)
			if got != (test.want == "show") {
				t.Fatalf("shouldShowFullDiskHint() = %v, want %s", got, test.want)
			}
		})
	}
}

func TestFullDiskHintViewUsesApprovedCopy(t *testing.T) {
	if !fullDiskAccessSupported() {
		t.Skip("full disk access hint is macOS-only")
	}
	for _, page := range []string{"Devices", "Settings"} {
		t.Run(page, func(t *testing.T) { testFullDiskHintView(t, page) })
	}
}

func testFullDiskHintView(t *testing.T, page string) {
	a := fixture()
	a.page, a.model.Pairing = page, nil
	a.model.ClipboardSync = true
	a.fullDiskGranted = false
	granted := false
	a.fullDisk = func() bool { return granted }
	tt := ui.NewTester(a.view, minWindowWidth, 900)
	for _, text := range []string{"Full Disk Access", "Needed to sync copied image files.", "Open settings", "Check again"} {
		if !tt.HasText(text) {
			t.Fatalf("missing full disk access hint text %q", text)
		}
	}
	granted = true
	clickRelease(t, tt, "Check again")
	if tt.HasText("Full Disk Access") {
		t.Fatal("full disk access hint remained after access was granted")
	}
}
