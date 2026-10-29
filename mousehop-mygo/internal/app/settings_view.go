package app

import (
	"github.com/egoist/mygo"
	"github.com/egoist/mygo/ui"
)

const fullDiskAccessPane = "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles"

func (a *application) settingsView(c *ui.Context) {
	a.fullDiskView(c)
	a.emailView(c)
	if (a.model.Capture != "Enabled" || a.model.Emulation != "Enabled") && (a.permissions == nil || !a.permissions.Visible()) {
		card(c.Key("input-status"), tr("Input access"), func() {
			if a.model.Capture != "Enabled" {
				if quietButton(c, tr("Enable input capture")).Disabled(!a.model.Online).Clicked() {
					a.send("EnableCapture", nil)
				}
			}
			if a.model.Emulation != "Enabled" {
				if quietButton(c, tr("Enable input emulation")).Disabled(!a.model.Online).Clicked() {
					a.send("EnableEmulation", nil)
				}
			}
		})
	}
	ui.Collapsible(c.Key("advanced"), tr("Advanced"), &a.advancedOpen, func() {
		ui.Column(c).Gap(12).Children(func() { a.advancedSettingsView(c) })
	})
	if !a.advancedOpen {
		a.releaseEditor.Open = false
	}
}

func (a *application) fullDiskView(c *ui.Context) {
	if !shouldShowFullDiskHint(a.model.ClipboardSync, a.fullDiskGranted, fullDiskAccessSupported()) {
		return
	}
	card(c.Key("full-disk"), tr("Full Disk Access"), func() {
		ui.Text(c, tr("Needed to sync copied image files.")).FontSize(12).TextColor(muted)
		ui.Row(c).Gap(8).Children(func() {
			if ui.PrimaryButton(c, tr("Open settings")).Clicked() {
				if err := mygo.Shell.OpenExternal(fullDiskAccessPane); err != nil {
					a.model.Error = err.Error()
				}
			}
			if ui.Button(c, tr("Check again")).Clicked() {
				a.refreshFullDiskAccess()
			}
		})
	})
}

func shouldShowFullDiskHint(clipboardSync, granted, supported bool) bool {
	return supported && clipboardSync && !granted
}

func (a *application) advancedSettingsView(c *ui.Context) {
	card(c.Key("global-options"), "", func() {
		a.globalToggle(c.Key("global-kcp"), tr("Reliable input (KCP)"), a.model.GlobalKCP, "SetGlobalKcp")
		a.globalToggle(c.Key("clipboard-sync"), tr("Clipboard sync"), a.model.ClipboardSync, "SetClipboardSync")
	})
	a.releaseView(c)
	card(c.Key("network"), tr("Network"), func() {
		ui.Row(c).Gap(10).AlignItems(ui.Center).Children(func() {
			ui.Text(c, tr("Listen port")).Grow(1)
			ui.NumberInput(c, &a.newPort, 1, 65535, 1).Label(tr("Listen port")).Width(104)
			if quietButton(c, tr("Apply port")).Disabled(!a.model.Online).Clicked() {
				a.send("ChangePort", int(a.newPort))
			}
		})
	})
	card(c.Key("privacy"), tr("Clipboard privacy"), func() {
		for _, app := range a.model.Suppressed {
			ui.Row(c.Key(app)).Gap(10).Children(func() {
				ui.Text(c, app).Grow(1)
				if quietButton(c, tr("Remove")).Disabled(!a.model.Online).Clicked() {
					a.send("RemoveSuppressedApp", app)
				}
			})
		}
		ui.TextInput(c, &a.suppressedDraft).Label(tr("Private application")).Placeholder(tr("Application identifier")).MinWidth(0)
		ui.Row(c).Gap(10).Children(func() {
			ui.Spacer(c)
			if quietButton(c, tr("Exclude app")).Disabled(!a.model.Online || a.suppressedDraft == "").Clicked() {
				a.send("AddSuppressedApp", a.suppressedDraft)
				a.suppressedDraft = ""
			}
			if quietButton(c, tr("Running apps")).Disabled(!a.model.Online).Clicked() {
				a.send("ListRunningApps", nil)
			}
		})
		for _, app := range a.model.Running {
			if quietButton(c.Key(app.Identifier), app.Name).Disabled(!a.model.Online).Clicked() {
				a.send("AddSuppressedApp", app.Identifier)
			}
		}
	})
}
