package app

import (
	_ "embed"

	"github.com/egoist/mygo/ui"
)

//go:generate go -C ../.. run ../scripts/generate-mygo-icons.go

//go:embed assets/app.png
var applicationIcon []byte

// Decode the embedded application artwork once, not on every UI frame.
var toolbarIcon = func() *ui.Bitmap {
	bitmap, err := ui.DecodeBitmap(applicationIcon)
	if err != nil {
		panic("invalid embedded MotionDuo application icon: " + err.Error())
	}
	return bitmap
}()

//go:embed assets/menubar.png
var menuBarIcon []byte

func trayIconForPlatform(platform string) []byte {
	if platform == "darwin" {
		return menuBarIcon
	}
	return applicationIcon
}
