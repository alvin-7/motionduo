package app

import (
	"math"
	"runtime"
	"slices"
	"time"

	"github.com/egoist/mygo/ui"
)

// Saves keep their confirmation deadline even when Advanced is collapsed.
func (a *application) syncRelease(c *ui.Context) {
	if pending := a.releasePending; pending != nil {
		switch {
		case SameBind(a.model.ReleaseBind, pending.Target), !SameBind(a.model.ReleaseBind, pending.Before):
			a.releasePending = nil
		case !c.Now().Before(pending.Until):
			a.releasePending = nil
			a.model.Error = tr("Shortcut was not confirmed; try again")
			c.Invalidate()
		default:
			c.After(pending.Until.Sub(c.Now()))
		}
	}
	if a.model.ThresholdLoaded {
		if !a.thresholdEditor.Sync(a.model.ReleaseThreshold, c.Now()) {
			a.model.Error = tr("Release threshold was not confirmed; try again")
			c.Invalidate()
		}
	}
	if pending := a.thresholdEditor.Pending; pending != nil {
		c.After(pending.Until.Sub(c.Now()))
	}
}

func (a *application) releaseView(c *ui.Context) {
	a.syncRelease(c)
	card(c.Key("release"), tr("Return control"), func() {
		ui.Row(c).Gap(12).AlignItems(ui.Center).Children(func() {
			label := tr("Waiting for daemon")
			if a.model.ReleaseBindLoaded {
				label = ChordLabel(a.model.ReleaseBind, runtime.GOOS)
			}
			ui.Text(c, label).Grow(1).MinWidth(0)
			if a.releasePending != nil {
				ui.Text(c, tr("Saving...")).TextColor(muted)
			}
			if quietButton(c, tr("Edit shortcut")).Disabled(!a.model.Online || !a.model.ReleaseBindLoaded || a.releasePending != nil).Clicked() {
				a.releaseEditor = NewShortcutEditor(a.model.ReleaseBind)
			}
		})
		if a.releaseEditor.Open {
			a.releaseShortcutView(c)
		}
		ui.Divider(c)
		e := &a.thresholdEditor
		ui.Text(c, tr("Wall-press fallback (px)"))
		ui.Row(c).Gap(12).AlignItems(ui.Center).Children(func() {
			ui.NumberInput(c, &e.Value, 0, math.MaxUint32, 1).Label(tr("Wall-press fallback pixels")).Width(160).Disabled(!a.model.Online || !a.model.ThresholdLoaded || e.Pending != nil)
			ui.Spacer(c)
			value, valid := ThresholdValue(e.Value)
			label := tr("Apply threshold")
			if e.Pending != nil {
				label = tr("Saving...")
			}
			if quietButton(c.Key("apply-threshold"), label).Disabled(!a.model.Online || !a.model.ThresholdLoaded || !valid || value == a.model.ReleaseThreshold || e.Pending != nil).Clicked() {
				if a.send("SetReleaseThreshold", value) {
					e.Pending = &PendingThreshold{value, c.Now().Add(3 * time.Second)}
					c.After(3 * time.Second)
				}
			}
		})
		ui.Text(c, tr("0 disables. Uses estimated motion; increase if control returns too early.")).FontSize(12).TextColor(muted)
	})
}

func (a *application) releaseShortcutView(c *ui.Context) {
	e := &a.releaseEditor
	ui.Column(c.Key("shortcut-editor")).Gap(12).Padding(16).Radius(8).Background(background).Disabled(!a.model.Online).Children(func() {
		for row := 0; row < len(Modifiers); row += 2 {
			ui.Row(c.Key(Modifiers[row].Code)).Gap(16).Children(func() {
				for i := row; i < row+2; i++ {
					label := KeyLabel(Modifiers[i].Code, runtime.GOOS)
					ui.Checkbox(c.Key(Modifiers[i].Code), &e.Modifiers[i], label).Label(trf("%s modifier", label)).Grow(1)
				}
			})
		}
		ui.Combobox(c, &e.Trigger, e.Options()).Label(tr("Trigger key"))
		keys := e.Keys()
		ui.Row(c).Gap(10).Children(func() {
			if accentButton(c, tr("Apply shortcut")).Disabled(len(keys) == 0 || a.releasePending != nil).Clicked() {
				a.saveReleaseShortcut(c, keys, keys)
			}
			if quietButton(c, tr("Cancel shortcut")).Clicked() {
				e.Open = false
			}
			ui.Spacer(c)
			if quietButton(c, tr("Use default")).Disabled(a.releasePending != nil).Clicked() {
				// An empty IPC chord restores the daemon default; it never disables release.
				a.saveReleaseShortcut(c, []string{}, DefaultBind)
			}
		})
	})
}

func (a *application) saveReleaseShortcut(c *ui.Context, request, expected []string) {
	if a.send("SetReleaseBind", request) {
		a.releaseEditor.Open = false
		a.releasePending = &PendingShortcut{slices.Clone(a.model.ReleaseBind), slices.Clone(expected), c.Now().Add(3 * time.Second)}
		c.After(3 * time.Second)
	}
}
