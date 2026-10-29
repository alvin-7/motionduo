package app

import (
	"log"
	"os"
	"testing"
	"time"

	"github.com/alvin-7/mousehop/mygo/internal/locale"

	"github.com/egoist/mygo"
	"github.com/egoist/mygo/ui"
)

var nativeVisualProbe = func() {}

// Opt-in native rendering fixture: no IPC, tray, config writes or input hooks.
func TestMain(m *testing.M) {
	locale.Language = "en" // Label tests must not depend on the host UI language.
	if os.Getenv("MOUSEHOP_NATIVE_VISUAL") != "1" {
		os.Exit(m.Run())
	}
	a := fixture()
	a.model.Pairing = nil
	mygo.App.SetName("Mousehop visual fixture")
	mygo.Theme.SetSource(mygo.ThemeDark)
	mygo.App.WhenReady(func() {
		nativeVisualProbe()
		logged := false
		a.win = mygo.NewWindow(mygo.WindowOptions{
			Title: "Mousehop visual fixture (isolated)", Width: windowWidth, Height: windowHeight,
			MinWidth: minWindowWidth, MinHeight: minWindowHeight, Content: ui.View(func(c *ui.Context) {
				ui.Box(c).Fill().DrawOver(func(p *ui.Painter, r ui.Rect) {
					if !logged {
						log.Printf("native surface: %.0fx%.0f DIP, scale=%.3f, accelerated=%t", r.W, r.H, p.Scale(), p.Accelerated())
						logged = true
					}
				}).Children(func() { a.view(c) })
			}),
		})
		time.AfterFunc(5*time.Minute, mygo.App.Quit)
	})
	if err := mygo.App.Run(); err != nil {
		panic(err)
	}
}
