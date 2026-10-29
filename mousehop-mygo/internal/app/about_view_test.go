package app

import (
	"testing"

	"github.com/alvin-7/mousehop/mygo/internal/locale"
	"github.com/egoist/mygo/ui"
)

func TestAboutAndBottomServiceStatus(t *testing.T) {
	for _, lang := range []string{"en", "zh-Hans"} {
		t.Run(lang, func(t *testing.T) {
			old := locale.Language
			locale.Language = lang
			defer func() { locale.Language = old }()
			a := fixture()
			a.model.Pairing = nil
			a.page = "About"
			tt := ui.NewTester(a.view, minWindowWidth, minWindowHeight)
			for _, text := range []string{tr("About"), "MotionDuo", trf("Version %s", versionWithCommit()), "alvin-7/motionduo", "alvin-7", tr("Based on Mousehop and lan-mouse"), tr("GPL-3.0-or-later; upstream copyrights are preserved.")} {
				if !tt.HasText(text) {
					t.Fatal("missing about information", text, tt.Texts())
				}
			}
			r := findScreen(t, tt, tr("Ready"))
			if r.Y < minWindowHeight-30 {
				t.Fatal("service status is not at bottom", r)
			}
		})
	}
}

func TestVersionWithInjectedCommit(t *testing.T) {
	old := commit
	defer func() { commit = old }()
	commit = "abcdef012345-dirty"
	if got := versionWithCommit(); got != version+" (abcdef0-dirty)" {
		t.Fatal(got)
	}
}
