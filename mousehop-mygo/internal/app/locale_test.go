package app

import (
	"testing"

	"github.com/alvin-7/mousehop/mygo/internal/locale"
	"github.com/egoist/mygo/ui"
)

func useLanguage(t *testing.T, language string) {
	t.Helper()
	before := locale.Language
	locale.Language = language
	t.Cleanup(func() { locale.Language = before })
}

func TestChinesePagesAndProtocolValues(t *testing.T) {
	useLanguage(t, "zh-Hans")
	for _, page := range []string{"Devices", "Screen layout", "Settings"} {
		a := fixture()
		a.page, a.model.Pairing = page, nil
		tt := ui.NewTester(a.view, minWindowWidth, minWindowHeight)
		if !tt.HasText(tr(page)) || !tt.HasText("MotionDuo") || !tt.HasText("Studio-PC") {
			t.Fatal(page, tt.Texts())
		}
	}
	a := fixture()
	var kind string
	var value any
	a.request = func(k string, v any) error { kind, value = k, v; return nil }
	tt := ui.NewTester(a.view, minWindowWidth, minWindowHeight)
	clickRelease(t, tt, "允许")
	if kind != "RespondPairing" {
		t.Fatal(kind, value)
	}
	if tr("unknown diagnostic") != "unknown diagnostic" || trf("Share with %s", "MacBook Pro") != "与 MacBook Pro 共享" {
		t.Fatal("fallback or device identity changed")
	}
	for key, value := range locale.SimplifiedChinese {
		if key == "" || value == "" {
			t.Fatal("empty dictionary entry")
		}
	}
}
