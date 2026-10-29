package app

import (
	"runtime/debug"
	"strings"

	"github.com/egoist/mygo/ui"
)

var commit string

func versionWithCommit() string {
	revision := commit
	if revision == "" {
		if info, ok := debug.ReadBuildInfo(); ok {
			var dirty bool
			for _, setting := range info.Settings {
				switch setting.Key {
				case "vcs.revision":
					revision = setting.Value
				case "vcs.modified":
					dirty = setting.Value == "true"
				}
			}
			if revision != "" && dirty {
				revision += "-dirty"
			}
		}
	}
	if revision == "" {
		return version
	}
	dirty := strings.HasSuffix(revision, "-dirty")
	revision = strings.TrimSuffix(revision, "-dirty")
	if len(revision) > 7 {
		revision = revision[:7]
	}
	if dirty {
		revision += "-dirty"
	}
	return version + " (" + revision + ")"
}

func (a *application) aboutView(c *ui.Context) {
	card(c.Key("about"), "", func() {
		ui.Row(c).Gap(12).AlignItems(ui.Center).Children(func() {
			ui.Image(c, toolbarIcon).Size(44, 44).Shrink(0)
			ui.Column(c).Gap(4).Children(func() {
				ui.Text(c, "MotionDuo").FontSize(20).Bold()
				ui.Text(c, trf("Version %s", versionWithCommit())).FontSize(12).TextColor(muted)
			})
		})
		ui.Divider(c)
		ui.Link(c, "alvin-7/motionduo", "https://github.com/alvin-7/motionduo")
		ui.Row(c).Children(func() {
			ui.Text(c, tr("Maintainer")).Grow(1).TextColor(muted)
			ui.Link(c, "alvin-7", "https://github.com/alvin-7")
		})
		ui.Divider(c)
		ui.Text(c, tr("Based on Mousehop and lan-mouse")).FontSize(12).TextColor(muted)
		ui.Link(c, "Mousehop · jondkinney", "https://github.com/jondkinney/mousehop")
		ui.Link(c, "lan-mouse · Ferdinand Schober & contributors", "https://github.com/feschber/lan-mouse")
		ui.Text(c, tr("GPL-3.0-or-later; upstream copyrights are preserved.")).FontSize(12).TextColor(muted)
	})
}
