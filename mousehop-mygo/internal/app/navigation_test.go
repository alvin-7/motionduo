package app

import (
	"testing"

	"github.com/egoist/mygo/ui"
)

func TestIntegratedToolbarReservesNativeButtons(t *testing.T) {
	for _, bar := range []ui.TitleBar{{Height: 50, Right: 138}, {Height: 50, Left: 80}} {
		a := fixture()
		a.model.Pairing = nil
		tt := ui.NewTester(a.view, minWindowWidth, minWindowHeight)
		tt.SetTitleBar(bar)
		tt.Frame()
		for _, label := range []string{"Devices", "Screen layout", "Settings"} {
			r := findScreen(t, tt, label)
			if r.Y < bar.Height || r.X < 14 || r.X+r.W > float32(minWindowWidth)-14 {
				t.Fatalf("%s overlaps native controls: %+v, %+v", label, r, bar)
			}
		}
		tt.Click("Settings")
		if a.page != "Settings" {
			t.Fatal("segmented navigation did not change page")
		}
	}
}
