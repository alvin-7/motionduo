package app

import (
	"runtime"
	"strings"
	"time"

	"github.com/egoist/mygo/ui"
)

// Neutral graphite: opaque surfaces keep native text crisp without a blue cast.
var (
	textPrimary    = ui.Hex("#ededf0")
	muted          = ui.Hex("#b0afb8")
	border         = ui.Hex("#3b3a40")
	background     = ui.Hex("#222225")
	surface        = ui.Hex("#29292d")
	surfaceHover   = ui.Hex("#323237")
	surfacePressed = ui.Hex("#3b3b41")
	blue           = ui.Hex("#9bbffc")
	green          = ui.Hex("#75c8ab")
)

func graphiteTheme() ui.Theme {
	t := *ui.DarkTheme()
	t.Background, t.Surface, t.Text, t.TextMuted = background, surface, textPrimary, muted
	t.SurfaceHover, t.SurfacePressed = surfaceHover, surfacePressed
	t.Border, t.Accent, t.AccentHover, t.AccentPressed = border, blue, ui.Hex("#b8d2ff"), ui.Hex("#7da8ed")
	t.AccentText, t.Focus, t.Radius, t.FontSize = background, blue, 8, 13
	t.Inverse, t.InverseText = surfacePressed, textPrimary
	if runtime.GOOS == "windows" {
		t.Font = "Segoe UI"
	}
	return t
}

func (a *application) view(c *ui.Context) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if a.page != "Screen layout" || !a.model.Online {
		a.layoutGesture = layoutGesture{}
	}
	if !a.model.Online {
		a.manualOpen = false
		a.layoutPending = nil
		a.releasePending = nil
		a.thresholdEditor = ThresholdEditor{}
		a.connectionEditors = nil
		a.configuredDevice = ""
	}
	if a.page != "Devices" {
		a.manualOpen = false
		a.closeDeviceDrafts()
	}
	if a.page != "Settings" || !a.model.Online {
		a.releaseEditor.Open = false
	}
	t := graphiteTheme()
	c.SetTheme(&t)
	a.syncEmail(c)
	a.syncDiscovery(c)
	if a.model.Online && !a.portInitialized {
		a.newPort = float64(a.model.Port)
		a.portInitialized = true
	}
	a.syncRelease(c)
	a.syncLayout(c)
	for id, editor := range a.connectionEditors {
		if peer, ok := a.model.Clients[id]; ok {
			a.syncConnection(c, peer, editor)
		}
	}
	ui.Column(c).Fill().Background(background).Children(func() {
		bar := c.TitleBar()
		ui.Row(c.Key("toolbar")).Height(max(36, bar.Height)).Shrink(0).Padding(0, max(14, bar.Right+8), 0, max(14, bar.Left+8)).Gap(10).AlignItems(ui.Center).Background(background).DragWindow().Children(func() {
			ui.Image(c.Key("application-icon"), toolbarIcon).Label("MotionDuo").Size(22, 22).Shrink(0)
			ui.Text(c, "MotionDuo").FontSize(15).Bold()
		})
		ui.Row(c.Key("navigation-row")).Height(42).Shrink(0).Padding(0, 14).AlignItems(ui.Center).Children(func() {
			a.navigation(c)
			ui.Spacer(c)
			ui.Text(c, a.model.DeviceName).FontSize(12).TextColor(muted).MinWidth(0).MaxLines(1).Tooltip(a.model.DeviceName)
		})
		ui.Column(c.Key("main")).Grow(1).MinHeight(0).Padding(14).Gap(12).Children(func() {
			if a.model.Error != "" {
				ui.Row(c.Key("error")).Padding(12).Gap(8).Radius(8).Background(ui.Hex("#35252a")).Children(func() {
					ui.Text(c, tr(a.model.Error)).Grow(1).TextColor(ui.Hex("#f09b89"))
					if quietButton(c, tr("Dismiss")).Clicked() {
						a.model.Error = ""
					}
				})
			}
			if len(a.model.Pairing) > 0 {
				// Requests stay visible even when the settings content is scrolled.
				ui.Scroll(c.Key("requests")).MaxHeight(144).Shrink(0).Gap(8).Children(func() { a.pairingView(c) })
			}
			var content ui.Element
			if a.page == "Screen layout" {
				content = ui.Column(c.Key("layout-content")).Grow(1).MinHeight(0).Gap(12)
			} else {
				content = ui.Scroll(c.Key("content")).Grow(1).Gap(12)
			}
			content.Children(func() {
				a.permissionView(c)
				switch a.page {
				case "Devices":
					a.fullDiskView(c)
					a.devicesView(c)
				case "Screen layout":
					a.layoutView(c)
				case "Settings":
					a.settingsView(c)
				case "About":
					a.aboutView(c)
				}
			})
		})
		ui.Row(c.Key("service-status-bar")).Height(24).Shrink(0).Padding(0, 14).Gap(6).AlignItems(ui.Center).Children(func() {
			status, color := tr("Reconnecting"), muted
			if a.model.Online {
				status, color = tr("Ready"), green
			}
			if a.permissions != nil && a.permissions.Visible() {
				status, color = tr("Input access needed"), ui.Hex("#e9b866")
				if a.permissions.RestartRequired && a.permissions.Err == nil {
					status = tr("Restart needed")
				}
			}
			ui.Box(c.Key("service-status")).Label(status).Size(5, 5).Radius(3).Background(color)
			ui.Text(c, status).FontSize(11).TextColor(muted)
		})
	})
	a.manualConnectionView(c)
}

func card(c *ui.Context, title string, build func()) {
	ui.Column(c).Padding(14).Gap(12).Radius(10).Border(0.5, border).Background(surface).InsetShadow(0, 0.5, 0, 0, ui.RGBA(255, 255, 255, 0.035)).Shadow(0, 2, 5, -2, ui.RGBA(0, 0, 0, 0.22)).Children(func() {
		if title != "" {
			ui.Text(c, title).FontSize(13).Bold()
		}
		build()
	})
}

func (a *application) emailView(c *ui.Context) {
	card(c.Key("email"), tr("Discovery email"), func() {
		mdns := a.model.MDNS
		ui.Row(c).Height(28).AlignItems(ui.Center).Children(func() {
			ui.Text(c, tr("Local discovery")).Grow(1)
			if ui.Switch(c, &mdns).Label(tr("Local discovery")).Disabled(!a.model.Online).Changed() {
				a.send("SetMdnsDiscovery", mdns)
			}
		})
		ui.Divider(c)
		ui.Row(c).Gap(10).AlignItems(ui.Center).Children(func() {
			input := ui.TextInput(c, &a.emailDraft).Label(tr("Email address")).Placeholder("you@example.com").Grow(1).Disabled(a.emailPending != nil)
			label := tr("Save email")
			unchanged := normalizedEmail(a.emailDraft) == a.model.Email
			if a.emailPending != nil {
				label = tr("Saving...")
			} else if unchanged && c.Now().Before(a.emailSavedUntil) {
				label = tr("Saved")
			}
			disabled := !a.model.Online || !a.model.EmailLoaded || a.emailPending != nil || unchanged
			if (accentButton(c, label).Disabled(disabled).Clicked() || input.Submitted()) && !disabled {
				target := normalizedEmail(a.emailDraft)
				if a.send("SetDiscoveryEmail", target) {
					a.emailPending = &pendingEmail{a.model.Email, target, c.Now().Add(3 * time.Second), a.model.ErrorSequence}
					c.After(3 * time.Second)
				}
			}
		})
		ui.Text(c, tr("Use the same email on your computers. No account needed.")).FontSize(12).TextColor(muted)
	})
}

func (a *application) pairingView(c *ui.Context) {
	for _, request := range a.model.Pairing {
		ui.Row(c.Key("pair-"+request.ID)).Gap(10).Padding(12).Radius(10).Border(1, ui.Hex("#405b80")).Background(ui.Hex("#202e42")).AlignItems(ui.Center).Children(func() {
			ui.Column(c).Grow(1).MinWidth(0).Gap(5).Children(func() {
				ui.Text(c, trf("%s requests control of this computer", request.Name)).Bold().MaxLines(2)
				if len(request.Addresses) > 0 {
					ui.Text(c, strings.Join(request.Addresses, " · ")).FontSize(12).TextColor(muted)
				}
			})
			if quietButton(c.Key("reject"), tr("Decline")).Disabled(!a.model.Online).Clicked() {
				a.send("RespondPairing", []any{request.ID, false})
			}
			if accentButton(c.Key("approve"), tr("Allow")).Disabled(!a.model.Online).Clicked() {
				a.send("RespondPairing", []any{request.ID, true})
			}
		})
	}
}
