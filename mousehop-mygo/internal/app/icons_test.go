package app

import (
	"bytes"
	"encoding/binary"
	"image"
	"image/color"
	"image/draw"
	"image/png"
	"os"
	"path/filepath"
	"testing"
)

func TestApplicationIconMatchesMotionDuoWindowsAsset(t *testing.T) {
	source, err := os.ReadFile("../../../assets/brand/motionduo.ico")
	if err != nil {
		t.Fatal(err)
	}
	img, err := png.Decode(bytes.NewReader(applicationIcon))
	if err != nil || img.Bounds().Dx() != 256 || img.Bounds().Dy() != 256 {
		t.Fatal("invalid application artwork", err)
	}
	if len(source) < 6 || binary.LittleEndian.Uint16(source[2:]) != 1 {
		t.Fatal("invalid ICO")
	}
	matched := false
	for i := 0; i < int(binary.LittleEndian.Uint16(source[4:])); i++ {
		at := 6 + 16*i
		if at+16 > len(source) {
			t.Fatal("truncated ICO directory")
		}
		length, offset := int(binary.LittleEndian.Uint32(source[at+8:])), int(binary.LittleEndian.Uint32(source[at+12:]))
		if offset > len(source) || length > len(source)-offset {
			t.Fatal("truncated ICO image")
		}
		frame, err := png.Decode(bytes.NewReader(source[offset : offset+length]))
		if err == nil && frame.Bounds().Dx() == 256 && frame.Bounds().Dy() == 256 {
			matched = true
			for y := 0; y < 256; y++ {
				for x := 0; x < 256; x++ {
					r, g, b, a := img.At(x, y).RGBA()
					r2, g2, b2, a2 := frame.At(x, y).RGBA()
					if absDiff(r, r2) > 1024 || absDiff(g, g2) > 1024 || absDiff(b, b2) > 1024 || absDiff(a, a2) > 1024 {
						matched = false
					}
				}
			}
		}
	}
	if !matched {
		t.Fatal("ICO artwork differs from MotionDuo application icon")
	}
	if !bytes.Equal(trayIconForPlatform("windows"), applicationIcon) || !bytes.Equal(trayIconForPlatform("darwin"), menuBarIcon) {
		t.Fatal("platform resident icon selection is inconsistent")
	}
}

func absDiff(a, b uint32) uint32 {
	if a > b {
		return a - b
	}
	return b - a
}

func TestToolbarUsesDecodedApplicationArtwork(t *testing.T) {
	if toolbarIcon == nil {
		t.Fatal("toolbar application bitmap is missing")
	}
	if w, h := toolbarIcon.Size(); w != 256 || h != 256 {
		t.Fatalf("toolbar must use the canonical full-size artwork, got %dx%d", w, h)
	}
}

func TestMenuBarTemplateHasOnlyBlackAndTransparency(t *testing.T) {
	img, err := png.Decode(bytes.NewReader(menuBarIcon))
	if err != nil {
		t.Fatal(err)
	}
	if img.Bounds().Dx() != 64 || img.Bounds().Dy() != 64 {
		t.Fatal("menu-bar template is not a Retina-ready square")
	}
	opaque, clear := 0, 0
	for y := 0; y < 64; y++ {
		for x := 0; x < 64; x++ {
			r, g, b, alpha := img.At(x, y).RGBA()
			if r != 0 || g != 0 || b != 0 {
				t.Fatal("template contains an opaque colored background")
			}
			if alpha == 0 {
				clear++
			} else {
				opaque++
			}
		}
	}
	if opaque < 200 || clear < 2000 {
		t.Fatal("template is empty or lacks transparent padding", opaque, clear)
	}
	if dir := os.Getenv("MOUSEHOP_TEST_SNAPSHOTS"); dir != "" {
		if err := os.MkdirAll(dir, 0755); err != nil {
			t.Fatal(err)
		}
		preview := image.NewRGBA(image.Rect(0, 0, 96, 96))
		draw.Draw(preview, preview.Bounds(), image.NewUniform(color.White), image.Point{}, draw.Src)
		draw.Draw(preview, image.Rect(16, 16, 80, 80), img, image.Point{}, draw.Over)
		file, err := os.Create(filepath.Join(dir, "menu-bar-template-preview.png"))
		if err != nil {
			t.Fatal(err)
		}
		defer file.Close()
		if err := png.Encode(file, preview); err != nil {
			t.Fatal(err)
		}
	}
}
