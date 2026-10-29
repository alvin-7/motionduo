package app

import (
	"fmt"
	"runtime"

	"github.com/alvin-7/mousehop/mygo/internal/model"
	"github.com/egoist/mygo/ui"
)

func clientDeviceKey(id uint64) string   { return fmt.Sprintf("client-%d", id) }
func incomingDeviceKey(id string) string { return "incoming-" + id }

func (a *application) closeDeviceDrafts() {
	for _, editor := range a.connectionEditors {
		editor.Editing = false
		editor.AliasEditing = false
	}
}

func (a *application) pruneDeviceSettings() {
	if a.configuredDevice == "" {
		return
	}
	for id := range a.model.Clients {
		if a.configuredDevice == clientDeviceKey(id) {
			return
		}
	}
	for id := range a.model.Incoming {
		if a.configuredDevice == incomingDeviceKey(id) {
			return
		}
	}
	a.configuredDevice = ""
}

func (a *application) deviceConfigButton(c *ui.Context, key, label string) ui.CollapsibleParts {
	open := a.configuredDevice == key
	parts := ui.CollapsibleBase(c.Key("configure"), &open)
	trigger := parts.Trigger.Label(label).Padding(7, 10).Gap(6).AlignItems(ui.Center).Radius(7).Border(0.5, ui.Hex("#48474e")).InsetShadow(0, 0.5, 0, 0, ui.RGBA(255, 255, 255, 0.065))
	fill, foreground := surface, textPrimary
	if open {
		fill, foreground = surfacePressed, textPrimary
	} else if trigger.Hovered() {
		fill = surfaceHover
	}
	if trigger.Pressed() {
		fill, foreground = surfacePressed, textPrimary
	}
	trigger.Background(fill).TextColor(foreground).Children(func() {
		ui.Text(c, tr("Configure")).SingleLine()
		ui.Box(c).Size(10, 10).Shrink(0).Draw(func(p *ui.Painter, r ui.Rect) {
			drawDisclosureChevron(p, r, parts.Progress(), foreground)
		})
	})
	if trigger.Changed() {
		a.closeDeviceDrafts()
		if open {
			a.configuredDevice = key
		} else if a.configuredDevice == key {
			a.configuredDevice = ""
		}
	}
	return parts
}

func (a *application) deviceSettingsView(c *ui.Context, peer model.Client) {
	a.aliasView(c.Key("device-alias"), peer)
	a.toggle(c.Key("inertia"), tr("Trackpad momentum"), peer.Config.ScrollInertia, "SetClientScrollInertia", peer.ID)
	if runtime.GOOS == "darwin" {
		a.toggle(c.Key("alias"), tr("Command as Control"), peer.Config.CommandAsCtrl, "SetClientCommandAsCtrl", peer.ID)
	}
	a.toggle(c.Key("crossing"), tr("Hold modifier to cross"), peer.Config.RequireModifier, "SetClientRequireCrossingModifier", peer.ID)
	if peer.Config.RequireModifier {
		modifier := peer.Config.Modifier
		if ui.Select(c, &modifier, []string{"shift", "control", "alt", "super"}).Label(tr("Crossing modifier")).Changed() {
			a.send("SetClientCrossingModifier", []any{peer.ID, modifier})
		}
	}
	a.connectionView(c, peer)
	if ui.Button(c, tr("Remove computer")).Disabled(!a.model.Online).Clicked() {
		a.send("Delete", peer.ID)
	}
}

func (a *application) incomingDevicesView(c *ui.Context) {
	if len(a.model.Incoming) == 0 {
		return
	}
	card(c.Key("incoming"), tr("Computers allowed to control you"), func() {
		for _, id := range sortedIncoming(a.model.Incoming) {
			peer := a.model.Incoming[id]
			key := incomingDeviceKey(id)
			ui.Column(c.Key(key)).Gap(12).Children(func() {
				var disclosure ui.CollapsibleParts
				ui.Row(c).Gap(10).AlignItems(ui.Center).Children(func() {
					name := peer.Description
					if name == "" {
						name = tr("Computer")
					}
					ui.Text(c, name).Bold().Grow(1).MinWidth(0).MaxLines(1)
					if a.model.RemoteStates[id] == "Locked" {
						ui.Text(c, tr("Locked - unlock locally")).FontSize(12).TextColor(muted)
					}
					disclosure = a.deviceConfigButton(c, key, trf("Configure incoming %s", name))
				})
				disclosure.Panel(func() {
					ui.Column(c.Key("configuration")).Padding(12).Gap(12).Radius(8).Background(background).Disabled(a.configuredDevice != key).Children(func() {
						a.toggle(c.Key("scroll"), tr("Natural scrolling"), peer.NaturalScroll, "SetIncomingPeerNaturalScroll", id)
						sensitivity := peer.Sensitivity
						ui.Row(c).Gap(12).AlignItems(ui.Center).Children(func() {
							ui.Text(c, tr("Pointer sensitivity")).Grow(1)
							if ui.NumberInput(c, &sensitivity, 0.1, 5, 0.1).Label(tr("Pointer sensitivity")).Width(160).Disabled(!a.model.Online).Changed() {
								a.send("SetIncomingPeerSensitivity", []any{id, sensitivity})
							}
						})
						if ui.Button(c, tr("Remove access")).Disabled(!a.model.Online).Clicked() {
							a.send("RemoveAuthorizedKey", id)
						}
					})
				})
			})
		}
	})
}
