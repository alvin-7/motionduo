package app

import (
	"strings"
	"time"

	"github.com/alvin-7/mousehop/mygo/internal/model"
	"github.com/egoist/mygo/ui"
)

func (a *application) syncConnection(c *ui.Context, peer model.Client, e *connectionEditor) {
	a.syncAlias(c, peer, e)
	actual := peerConnection(peer)
	if pending := e.Pending; pending != nil {
		switch {
		case sameConnection(actual, pending.Target), !sameConnection(actual, pending.Before):
			e.Pending = nil
		case !c.Now().Before(pending.Until):
			e.Pending = nil
			a.model.Error = tr("Connection settings were not confirmed; try again")
			c.Invalidate()
		default:
			c.After(pending.Until.Sub(c.Now()))
		}
	}
	current := networkSelection(peer)
	if pending := e.Network; pending != nil {
		switch {
		case current == pending.Target, current != pending.Before:
			e.Network = nil
		case !c.Now().Before(pending.Until):
			e.Network = nil
			a.model.Error = tr("Network choice was not confirmed; try again")
			c.Invalidate()
		default:
			c.After(pending.Until.Sub(c.Now()))
		}
	}
}

func (a *application) connectionView(c *ui.Context, peer model.Client) {
	e := a.connectionEditor(peer.ID)
	a.syncConnection(c, peer, e)
	actual := peerConnection(peer)
	if ui.Collapsible(c.Key("connection-details"), tr("Connection details"), &e.Open, func() {
		ui.Column(c).Padding(12, 0).Gap(12).Children(func() {
			status := tr("Not connected")
			if peer.State.Alive && peer.State.Address != nil {
				status = trf("Connected to %s", *peer.State.Address)
			}
			if peer.State.Resolving {
				status = tr("Resolving hostname...")
			}
			ui.Text(c, status).TextColor(muted).FontSize(12)
			a.networkChoiceView(c, peer, e)
			if peer.State.ActiveLock != nil {
				ui.Text(c, tr("Pinned on this network; no automatic failover.")).FontSize(12).TextColor(muted)
			}
			if e.Pending != nil {
				ui.Text(c, tr("Saving connection...")).TextColor(muted)
			}
			if !e.Editing {
				ui.Row(c).Gap(10).Children(func() {
					if ui.Button(c, tr("Edit connection")).Disabled(!a.model.Online || e.Pending != nil || e.Network != nil).Clicked() {
						e.Draft, e.Editing = draftConnection(actual), true
					}
					if ui.Button(c, tr("Refresh addresses")).Disabled(!a.model.Online || peer.Config.Hostname == nil || peer.State.Resolving || e.Pending != nil).Clicked() {
						a.send("ResolveDns", peer.ID)
					}
				})
			} else {
				a.connectionDraftView(c, peer, e)
			}
		})
	}).Label(trf("%s connection details", peer.Name())).Changed() && !e.Open {
		e.Editing = false
	}
}

func (a *application) connectionDraftView(c *ui.Context, peer model.Client, e *connectionEditor) {
	panel := ui.Column(c.Key("connection-editor")).Padding(16).Gap(12).Radius(8).Background(background).Disabled(!a.model.Online)
	panel.Children(func() {
		ui.TextInput(c, &e.Draft.Hostname).Label(tr("Remote hostname")).Placeholder(tr("Hostname (optional with fixed IPs)"))
		ui.Row(c).Gap(12).AlignItems(ui.Center).Children(func() {
			ui.Text(c, tr("Remote port")).Grow(1)
			ui.NumberInput(c, &e.Draft.Port, 1, 65535, 1).Label(tr("Connection port")).Width(160)
		})
		ui.TextInput(c, &e.Draft.IPs).Label(tr("Fixed IP addresses")).Placeholder(tr("Fixed IPv4 / IPv6 addresses, comma-separated"))
		ui.TextInput(c, &e.Draft.Hook).Label(tr("Enter command")).Placeholder(tr("Optional local command (sh -c)"))
		settings, err := e.Draft.settings()
		if err != nil {
			ui.Text(c, err.Error()).FontSize(12).TextColor(ui.Hex("#f09b89"))
		}
		ui.Row(c).Gap(10).Children(func() {
			if ui.PrimaryButton(c, tr("Save connection")).Disabled(err != nil || sameConnection(settings, peerConnection(peer)) || e.Pending != nil).Clicked() {
				if a.send("UpdateConnection", []any{peer.ID, settings}) {
					e.Pending = &pendingConnection{peerConnection(peer), settings, c.Now().Add(3 * time.Second)}
					e.Editing = false
					c.After(3 * time.Second)
				}
			}
			if ui.Button(c, tr("Cancel connection")).Clicked() {
				e.Editing = false
			}
		})
	})
	if panel.Shortcut(0, ui.KeyEscape) {
		e.Editing = false
		c.Invalidate()
	}
}

func (a *application) networkChoiceView(c *ui.Context, peer model.Client, e *connectionEditor) {
	choices, selected := networkChoices(peer), networkSelection(peer)
	label := tr("Prefer lowest latency")
	for _, choice := range choices {
		if choice.ID == selected {
			label = choice.Label
		}
	}
	if e.Network != nil {
		label += tr(" / Saving...")
	}
	sel := ui.SelectBase(c.Key("network-choice"), &selected)
	sel.Trigger.Label(tr("Network path")).Padding(7, 10).Radius(8).Border(1, border).Background(surface).Disabled(!a.model.Online || e.Network != nil || e.Pending != nil || e.Editing).Children(func() {
		ui.Row(c).Grow(1).MinWidth(0).Gap(10).AlignItems(ui.Center).Children(func() {
			ui.Text(c, label).Grow(1).MinWidth(0)
			ui.Box(c).Size(10, 10).Draw(func(p *ui.Painter, r ui.Rect) {
				drawDisclosureChevron(p, r, 1, muted)
			})
		})
	})
	sel.Popup(func(panel ui.Element) {
		panel.Padding(4).Radius(8).Border(1, border).Background(surface).Children(func() {
			ui.Scroll(c).MaxHeight(240).Children(func() {
				for _, choice := range choices {
					item := sel.Item(choice.ID).Label(choice.ID).Padding(8, 10).Radius(6)
					if item.Highlighted() {
						item.Background(blue).TextColor(background)
					}
					item.Children(func() { ui.Text(c, choice.Label) })
				}
			})
		})
	})
	if sel.Trigger.Changed() {
		kind, value := "SetConnectionMode", selected
		if ip, pinned := strings.CutPrefix(selected, "ip:"); pinned {
			kind, value = "SetNetworkLock", ip
		}
		if a.send(kind, []any{peer.ID, value}) {
			e.Network = &pendingNetworkChoice{networkSelection(peer), selected, c.Now().Add(10 * time.Second)}
			c.After(10 * time.Second)
		}
	}
}
