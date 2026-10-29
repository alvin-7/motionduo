package app

import (
	"fmt"
	"time"

	"github.com/egoist/mygo/ui"
)

func (a *application) layoutView(c *ui.Context) {
	clients := a.model.SortedClients()
	enabled := clients[:0]
	for _, peer := range clients {
		if peer.State.Active {
			enabled = append(enabled, peer)
		}
	}
	clients = enabled
	if peer, exists := a.model.Clients[a.selected]; !a.hasSelection || !exists || !peer.State.Active {
		a.hasSelection = false
		if len(clients) > 0 {
			a.selected, a.hasSelection = clients[0].ID, true
		}
	}
	w, _ := c.Size()
	width, height := a.layoutCanvas.W, a.layoutCanvas.H
	if width <= 0 || height <= 0 {
		width, height = w-28, layoutStageHeight
	}
	local, rects, _ := allScreenArrangement(width, height, clients, a.selected, a.layoutPending)
	if len(clients) == 0 {
		local, _ = screenArrangement(width, height, nil, nil, "right", "top_right")
	}
	g := &a.layoutGesture
	if g.Active {
		peer, ok := a.model.Clients[g.ID]
		if !ok || !peer.State.Active || !a.model.Online || g.Before != (screenPlacement{peer.Config.Position, peer.Config.Alignment}) || g.Width != width || g.Height != height {
			*g = layoutGesture{}
		}
	}
	stage := ui.Box(c.Key("layout-stage")).Label(tr("Screen arrangement")).Focusable().Grow(1).MinHeight(100).Radius(10).Border(0.5, border).Gradient(ui.Hex("#202023"), ui.Hex("#2b2b30"), 125).InsetShadow(0, 1, 5, 0, ui.RGBA(0, 0, 0, 0.18)).Clip()
	if g.Released {
		peer := a.model.Clients[g.ID]
		actual := screenPlacement{peer.Config.Position, peer.Config.Alignment}
		if g.Moved && !g.Outside && stage.Focused() && a.model.Online && g.Snap.Placement != actual {
			if other, occupied := a.layoutSideOccupied(peer, g.Snap.Placement.Position); occupied {
				a.model.Error = trf("Disable %s first", other.Name())
				c.Invalidate()
			} else if a.send("UpdateScreenPlacement", []any{peer.ID, g.Snap.Placement.Position, g.Snap.Placement.Alignment}) {
				a.layoutPending = &pendingPlacement{peer.ID, actual, g.Snap.Placement, c.Now().Add(3 * time.Second)}
				local, rects, _ = allScreenArrangement(width, height, clients, a.selected, a.layoutPending)
				c.After(3 * time.Second)
			}
		}
		*g = layoutGesture{}
	}
	if g.Active && g.Moved {
		rects[g.ID] = g.Shown
	}
	stage.HandleInput(func(ev ui.InputEvent) bool {
		a.mu.Lock()
		defer a.mu.Unlock()
		if a.page != "Screen layout" {
			a.layoutGesture = layoutGesture{}
			return false
		}
		p := ui.Point{X: ev.X, Y: ev.Y}
		switch ev.Kind {
		case ui.InputPointerDown:
			if ev.Button != 0 {
				return false
			}
			for _, peer := range clients {
				guest := rects[peer.ID]
				if !pointInScreen(p, guest) {
					continue
				}
				a.selected, a.hasSelection = peer.ID, true
				a.layoutGesture = layoutGesture{}
				if a.model.Online && a.layoutPending == nil {
					_, _, peerTargets := allScreenArrangement(width, height, clients, peer.ID, a.layoutPending)
					a.layoutGesture = layoutGesture{Active: true, ID: peer.ID, Local: local, Original: guest, Raw: guest, Shown: guest, Start: p, Grab: ui.Point{X: p.X - guest.X, Y: p.Y - guest.Y}, Before: screenPlacement{peer.Config.Position, peer.Config.Alignment}, Width: width, Height: height, Targets: peerTargets}
				}
				return true
			}
		case ui.InputPointerMove:
			if a.layoutGesture.Active {
				a.layoutGesture.move(p)
				return true
			}
		case ui.InputPointerUp:
			if a.layoutGesture.Active {
				a.layoutGesture.move(p)
				a.layoutGesture.Released = true
				a.layoutGesture.Outside = !pointInScreen(p, ui.Rect{W: width, H: height})
				return true
			}
		case ui.InputKeyDown:
			if ev.Key == ui.KeyEscape && a.layoutGesture.Active {
				a.layoutGesture = layoutGesture{}
				return true
			}
		}
		return false
	})
	stage.Draw(func(p *ui.Painter, r ui.Rect) {
		a.mu.Lock()
		if a.layoutCanvas.W != r.W || a.layoutCanvas.H != r.H {
			a.layoutCanvas = ui.Rect{W: r.W, H: r.H}
			a.layoutGesture = layoutGesture{}
			c.Invalidate()
		}
		a.mu.Unlock()
		for x := r.X + 16; x < r.X+r.W; x += 20 {
			for y := r.Y + 16; y < r.Y+r.H; y += 20 {
				p.Fill(ui.Rect{X: x, Y: y, W: 1, H: 1}, ui.Hex("#36363b"), 0)
			}
		}
	}).Children(func() {
		if g.Active && g.Moved {
			snap := g.Snap.Rect
			color := blue
			if _, occupied := a.layoutSideOccupied(a.model.Clients[g.ID], g.Snap.Placement.Position); occupied {
				color = ui.Hex("#e9a36c")
			}
			ui.Box(c.Key("snap-preview")).Absolute().Left(snap.X).Top(snap.Y).Size(snap.W, snap.H).Radius(8).Border(2, color).Background(ui.Hex("#354f77")).Opacity(0.45)
			ui.Text(c.Key("snap-name"), alignmentLabel(g.Snap.Placement.Alignment)).Absolute().Left(16).Top(14).FontSize(12).TextColor(color)
		}
		layoutMonitor(c.Key("local-screen"), local, tr("This computer"), false, ui.CursorDefault, true)
		for _, peer := range clients {
			cursor := ui.CursorGrab
			if !a.model.Online {
				cursor = ui.CursorDefault
			} else if g.Active && g.ID == peer.ID {
				cursor = ui.CursorGrabbing
			}
			key := "remote-screen"
			label := tr("Remote screen")
			if len(clients) > 1 {
				key = fmt.Sprintf("remote-screen-%d", peer.ID)
				label = fmt.Sprintf("%s #%d", tr("Remote screen"), peer.ID)
			}
			layoutMonitorState(c.Key(key), rects[peer.ID], peer.Name(), label, a.hasSelection && a.selected == peer.ID, cursor, peer.State.Active)
		}
		ui.Row(c.Key("layout-caption")).Absolute().Left(14).Top(height - 30).Height(18).Gap(6).AlignItems(ui.Center).Children(func() {
			message := tr("Drag the blue screen to arrange")
			if len(clients) == 0 {
				message = tr("Connect a computer first")
			} else if a.layoutPending != nil {
				message = tr("Saving arrangement…")
			} else if !a.model.Online {
				message = tr("Reconnect to arrange screens")
			}
			ui.Text(c, message).FontSize(11).TextColor(muted)
		})
	})
}

func alignmentLabel(alignment string) string {
	switch alignment {
	case "top_left":
		return tr("Top left")
	case "top_right":
		return tr("Top right")
	case "bottom_left":
		return tr("Bottom left")
	default:
		return tr("Bottom right")
	}
}
