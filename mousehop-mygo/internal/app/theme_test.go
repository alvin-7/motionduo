package app

import (
	"fmt"
	"image"
	"image/png"
	"math"
	"os"
	"path/filepath"
	"testing"

	"github.com/egoist/mygo/ui"
)

func luminance(c ui.Color) float64 {
	channel := func(v uint8) float64 {
		x := float64(v) / 255
		if x <= 0.04045 {
			return x / 12.92
		}
		return math.Pow((x+0.055)/1.055, 2.4)
	}
	return 0.2126*channel(c.R) + 0.7152*channel(c.G) + 0.0722*channel(c.B)
}

func TestNightThemeHasOpaqueSurfacesAndReadableStates(t *testing.T) {
	theme := graphiteTheme()
	if !theme.Dark {
		t.Fatal("widgets must inherit dark state colors")
	}
	for _, c := range []ui.Color{theme.Background, theme.Surface, theme.SurfaceHover, theme.SurfacePressed} {
		if c.A != 255 || luminance(c) > 0.1 {
			t.Fatal("light or translucent surface", c)
		}
		for _, fg := range []ui.Color{theme.Text, theme.TextMuted} {
			if ratio := (luminance(fg) + 0.05) / (luminance(c) + 0.05); ratio < 4.5 {
				t.Fatal("insufficient text contrast", ratio)
			}
		}
	}
	if ratio := (luminance(theme.Accent) + 0.05) / (luminance(theme.AccentText) + 0.05); ratio < 4.5 {
		t.Fatal("insufficient primary button contrast", ratio)
	}
}

func TestDisclosureChevronHasDiagonalStrokesAtDisplayScales(t *testing.T) {
	for _, scale := range []float32{1, 1.25, 1.5, 2} {
		for _, progress := range []float32{0, 0.5, 1} {
			t.Run(fmt.Sprintf("%g/%g", scale, progress), func(t *testing.T) {
				tt := ui.NewTester(func(c *ui.Context) {
					ui.Box(c).Fill().Background(background).Children(func() {
						ui.Box(c).Size(14, 14).Draw(func(p *ui.Painter, r ui.Rect) { drawDisclosureChevron(p, r, progress, textPrimary) })
					})
				}, 20, 20)
				tt.SetScale(scale)
				im := tt.Image()
				bounds := image.Rectangle{}
				for y := 0; y < im.Bounds().Dy(); y++ {
					for x := 0; x < im.Bounds().Dx(); x++ {
						if im.RGBAAt(x, y).R > 150 {
							bounds = bounds.Union(image.Rect(x, y, x+1, y+1))
						}
					}
				}
				w, h := float32(bounds.Dx())/scale, float32(bounds.Dy())/scale
				if w < 3 || h < 3 || progress == 0 && w >= h || progress == 1 && w <= h {
					t.Fatal("chevron collapsed into axis-aligned bars", bounds)
				}
			})
		}
	}
}

func TestNightPagesAtDisplayScales(t *testing.T) {
	for _, scale := range []float32{1, 1.25, 1.5, 2} {
		for _, page := range []string{"Devices", "Screen layout", "Settings"} {
			for _, configured := range []bool{false, true} {
				a := fixture()
				a.page = page
				if configured {
					a.configuredDevice = clientDeviceKey(7)
					a.advancedOpen = true
				}
				tt := ui.NewTester(a.view, windowWidth, windowHeight)
				tt.SetScale(scale)
				if !tt.HasText(page) {
					t.Fatal("missing navigation", page)
				}
				im := tt.Image()
				if im.Bounds().Dx() != int(float32(windowWidth)*scale) {
					t.Fatal("frame is not rendered at native device resolution", im.Bounds(), scale)
				}
				if dir := os.Getenv("MOUSEHOP_TEST_SNAPSHOTS"); dir != "" {
					if err := os.MkdirAll(dir, 0755); err != nil {
						t.Fatal(err)
					}
					file, err := os.Create(filepath.Join(dir, fmt.Sprintf("night-%s-%g-%t.png", page, scale, configured)))
					if err != nil {
						t.Fatal(err)
					}
					err = png.Encode(file, im)
					_ = file.Close()
					if err != nil {
						t.Fatal(err)
					}
				}
			}
		}
	}
}
