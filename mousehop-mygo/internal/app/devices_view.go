package app

import (
	"sort"
	"time"

	"github.com/alvin-7/mousehop/mygo/internal/model"
	"github.com/egoist/mygo/ui"
)

func (a *application) devicesView(c *ui.Context) {
	clients := a.model.SortedClients()
	if len(clients) > 0 {
		card(c.Key("saved"), tr("Computers you control"), func() {
			for _, peer := range clients {
				key := clientDeviceKey(peer.ID)
				ui.Column(c.Key(key)).Gap(12).Children(func() {
					var disclosure ui.CollapsibleParts
					ui.Row(c).Gap(10).AlignItems(ui.Center).Children(func() {
						deviceIcon(c, peer.State.Alive)
						ui.Column(c).Grow(1).MinWidth(0).Gap(5).Children(func() {
							ui.Text(c, peer.Name()).Bold().MaxLines(1)
							status, failed := deviceConnectionStatus(peer)
							statusColor := muted
							if failed {
								statusColor = ui.Hex("#f09b89")
							}
							ui.Text(c, status).FontSize(12).TextColor(statusColor).MaxLines(2)
						})
						if peer.Config.ApprovalPaused {
							if accentButton(c, tr("Request again")).Disabled(!a.model.Online).Clicked() {
								a.send("RetryApproval", peer.ID)
							}
						}
						disclosure = a.deviceConfigButton(c, key, trf("Configure %s", peer.Name()))
						active := peer.State.Active
						if ui.Switch(c.Key("active"), &active).Label(trf("Share with %s", peer.Name())).Disabled(!a.model.Online).Changed() {
							a.setDeviceActive(peer, active)
						}
					})
					disclosure.Panel(func() {
						ui.Column(c.Key("configuration")).Padding(12).Gap(12).Radius(8).Background(background).Disabled(a.configuredDevice != key).Children(func() {
							a.deviceSettingsView(c, peer)
						})
					})
				})
			}
		})
	}
	card(c.Key("nearby"), "", func() {
		ui.Row(c).AlignItems(ui.Center).Children(func() {
			ui.Text(c, tr("Nearby")).Bold().Grow(1)
			if quietButton(c.Key("add-manual"), "+").Label(tr("Connect by address")).Disabled(!a.model.Online).Clicked() {
				a.manualOpen = true
				a.manualHost, a.manualPort = "", 4252
			}
		})
		count := 0
		for _, device := range a.model.Nearby {
			saved := false
			for _, peer := range clients {
				if matchesDiscovered(peer, device) {
					saved = true
				}
			}
			if saved {
				continue
			}
			count++
			ui.Row(c.Key(device.ID)).Gap(14).AlignItems(ui.Center).Children(func() {
				deviceIcon(c, false)
				ui.Column(c).Grow(1).MinWidth(0).Gap(5).Children(func() {
					ui.Text(c, device.Name).Bold().MaxLines(1)
					ui.Text(c, platformName(device.Platform)).FontSize(12).TextColor(muted)
				})
				_, pending := a.discoveryPending[device.ID]
				label := tr("Connect")
				if pending {
					label = tr("Connecting...")
				}
				if accentButton(c, label).Disabled(!a.model.Online || pending).Clicked() {
					if a.send("ConnectDiscovered", []any{device.ID, "right"}) {
						if a.discoveryPending == nil {
							a.discoveryPending = map[string]pendingDiscovery{}
						}
						a.discoveryPending[device.ID] = pendingDiscovery{Device: device, Until: c.Now().Add(3 * time.Second), ErrorSequence: a.model.ErrorSequence}
						c.After(3 * time.Second)
					}
				}
			})
		}
		if count == 0 {
			if a.model.Email == "" {
				if accentButton(c, tr("Set discovery email")).Clicked() {
					a.page = "Settings"
				}
				return
			}
			ui.Column(c).Gap(4).Children(func() {
				ui.Text(c, tr("No other computers yet")).TextColor(muted)
				text := tr("Open MotionDuo on another computer using the same email.")
				if !a.model.MDNS {
					text = tr("Enable local discovery in Settings.")
				}
				ui.Text(c, text).FontSize(12).TextColor(muted)
			})
		}
	})
	a.incomingDevicesView(c)
}

func (a *application) setDeviceActive(peer model.Client, active bool) {
	if active {
		if other, occupied := a.otherActiveAt(peer, peer.Config.Position); occupied {
			a.model.Error = trf("Disable %s first", other.Name())
			return
		}
	}
	a.send("Activate", []any{peer.ID, active})
}

func deviceIcon(c *ui.Context, online bool) {
	color := muted
	if online {
		color = green
	}
	ui.Box(c).Size(42, 38).Radius(8).Background(background).Draw(func(p *ui.Painter, r ui.Rect) {
		p.Stroke(ui.Rect{X: r.X + 7, Y: r.Y + 7, W: 28, H: 19}, color, 3, 1.5)
		p.Line(r.X+21, r.Y+27, r.X+21, r.Y+30, 1.5, color)
		p.Line(r.X+14, r.Y+31, r.X+28, r.Y+31, 1.5, color)
	})
}

func platformName(platform string) string {
	switch platform {
	case "macos", "darwin":
		return "macOS"
	case "windows":
		return "Windows"
	case "linux":
		return "Linux"
	}
	return tr("Computer")
}

func (a *application) globalToggle(c *ui.Context, text string, checked bool, request string) {
	ui.Row(c).AlignItems(ui.Center).Children(func() {
		ui.Text(c, text).Grow(1)
		if ui.Switch(c, &checked).Label(text).Disabled(!a.model.Online).Changed() {
			a.send(request, checked)
		}
	})
}

func (a *application) toggle(c *ui.Context, text string, checked bool, request string, id any) {
	ui.Row(c).AlignItems(ui.Center).Children(func() {
		ui.Text(c, text).Grow(1)
		if ui.Switch(c, &checked).Label(text).Disabled(!a.model.Online).Changed() {
			a.send(request, []any{id, checked})
		}
	})
}

func sortedIncoming(peers map[string]model.IncomingPeer) []string {
	keys := make([]string, 0, len(peers))
	for key := range peers {
		keys = append(keys, key)
	}
	sort.Slice(keys, func(i, j int) bool { return peers[keys[i]].Description < peers[keys[j]].Description })
	return keys
}
