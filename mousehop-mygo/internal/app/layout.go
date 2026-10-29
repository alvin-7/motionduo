package app

import (
	"time"

	"github.com/alvin-7/mousehop/mygo/internal/model"
	"github.com/egoist/mygo/ui"
)

const (
	layoutStageHeight float32 = 360
	screenGap         float32 = 6
	layoutNameHeight  float32 = 70
)

type screenPlacement struct {
	Position  string
	Alignment string
}

type screenSnap struct {
	Placement screenPlacement
	Rect      ui.Rect
	Distance2 float32
}

func screenArrangement(width, height float32, local, remote *[2]uint32, position, alignment string) (ui.Rect, ui.Rect) {
	// Thumbnails express placement, not a merged desktop's aspect ratio.
	// Physical display geometry remains authoritative in the daemon.
	l, r := [2]uint32{1600, 1000}, [2]uint32{1600, 1000}
	// Reserve all eight targets so the local screen never shifts during a drag.
	a, b := screenDragArrangement(width, height, ui.Rect{W: float32(l[0]), H: float32(l[1])}, ui.Rect{W: float32(r[0]), H: float32(r[1])})
	return a, placedScreen(a, b, screenPlacement{position, alignment})
}

// The workspace reserves all eight targets at one scale.
func screenDragArrangement(width, height float32, local, guest ui.Rect) (ui.Rect, ui.Rect) {
	a := ui.Rect{W: local.W, H: local.H}
	left := placedScreen(a, guest, screenPlacement{"left", "top_left"})
	top := placedScreen(a, guest, screenPlacement{"top", "top_left"})
	scale := min((width-32-2*screenGap)/(a.W+2*left.W), (height-layoutNameHeight-32-2*screenGap)/(a.H+2*top.H))
	a.W, a.H = a.W*scale, a.H*scale
	a.X, a.Y = (width-a.W)/2, (height-layoutNameHeight-a.H)/2
	return a, ui.Rect{W: guest.W * scale, H: guest.H * scale}
}

func placedScreen(a, b ui.Rect, placement screenPlacement) ui.Rect {
	// Normalize the shared edge for display only; retain the guest's aspect
	// ratio. Backend geometry and the saved alignment are not rescaled.
	scale := a.H / b.H
	if placement.Position == "top" || placement.Position == "bottom" {
		scale = a.W / b.W
	}
	b.W, b.H = b.W*scale, b.H*scale
	alignment := placement.Alignment
	cross := func(from, to float32, end bool) float32 {
		if alignment == "" || alignment == "proportional" {
			return (from - to) / 2
		}
		if end {
			return from - to
		}
		return 0
	}
	switch placement.Position {
	case "left":
		b.X = a.X - b.W - screenGap
		b.Y = a.Y + cross(a.H, b.H, alignment == "bottom_left" || alignment == "bottom_right")
	case "top":
		b.Y = a.Y - b.H - screenGap
		b.X = a.X + cross(a.W, b.W, alignment == "top_right" || alignment == "bottom_right")
	case "bottom":
		b.Y = a.Y + a.H + screenGap
		b.X = a.X + cross(a.W, b.W, alignment == "top_right" || alignment == "bottom_right")
	default:
		b.X = a.X + a.W + screenGap
		b.Y = a.Y + cross(a.H, b.H, alignment == "bottom_left" || alignment == "bottom_right")
	}
	return b
}

var screenSnapPlacements = []screenPlacement{
	{"left", "top_left"}, {"left", "bottom_left"},
	{"right", "top_right"}, {"right", "bottom_right"},
	{"top", "top_left"}, {"top", "top_right"},
	{"bottom", "bottom_left"}, {"bottom", "bottom_right"},
}

func nearestScreenSnap(local, moving ui.Rect, preferred screenPlacement, targets ...map[string]ui.Rect) screenSnap {
	best := screenSnap{Distance2: float32(1e30)}
	for _, placement := range screenSnapPlacements {
		r := placedScreen(local, moving, placement)
		if len(targets) > 0 {
			r = targets[0][placement.Position]
		}
		// Centers remain comparable when snapping changes the thumbnail size.
		dx, dy := r.X+r.W/2-moving.X-moving.W/2, r.Y+r.H/2-moving.Y-moving.H/2
		distance := dx*dx + dy*dy
		tied := abs(distance-best.Distance2) <= 0.01
		prefer := placement == preferred
		if placement.Position == best.Placement.Position {
			// Equal shared edges put both corner outlines in the same place.
			// A slight cross-axis drag chooses the visible top/bottom or left/right guide.
			offset := moving.Y + moving.H/2 - local.Y - local.H/2
			end := placement.Alignment == "bottom_left" || placement.Alignment == "bottom_right"
			if placement.Position == "top" || placement.Position == "bottom" {
				offset = moving.X + moving.W/2 - local.X - local.W/2
				end = placement.Alignment == "top_right" || placement.Alignment == "bottom_right"
			}
			if abs(offset) > 1 {
				prefer = (offset > 0) == end
			}
		}
		if distance < best.Distance2-0.01 || tied && prefer {
			best = screenSnap{placement, r, distance}
		}
	}
	return best
}

func abs(v float32) float32 { return max(v, -v) }

type layoutGesture struct {
	Active, Moved, Released, Outside bool
	ID                               uint64
	Local, Original, Raw, Shown      ui.Rect
	Start, Grab                      ui.Point
	Before                           screenPlacement
	Snap                             screenSnap
	Magnetic                         bool
	Width                            float32
	Height                           float32
	Targets                          map[string]ui.Rect
}

func (g *layoutGesture) move(p ui.Point) {
	if !g.Moved && abs(p.X-g.Start.X)+abs(p.Y-g.Start.Y) < 4 {
		return
	}
	g.Moved = true
	g.Raw = ui.Rect{X: p.X - g.Grab.X, Y: p.Y - g.Grab.Y, W: g.Original.W, H: g.Original.H}
	preferred := g.Before
	if g.Snap.Placement.Position != "" {
		preferred = g.Snap.Placement
	}
	if g.Targets == nil {
		g.Snap = nearestScreenSnap(g.Local, g.Raw, preferred)
	} else {
		g.Snap = nearestScreenSnap(g.Local, g.Raw, preferred, g.Targets)
	}
	g.Magnetic = g.Snap.Distance2 <= 18*18
	g.Shown = g.Raw
	if g.Magnetic {
		g.Shown = g.Snap.Rect
	}
}

type pendingPlacement struct {
	ID     uint64
	Before screenPlacement
	Target screenPlacement
	Until  time.Time
}

func (a *application) pruneLayoutState() {
	if a.hasSelection {
		if peer, exists := a.model.Clients[a.selected]; !exists || !peer.State.Active {
			a.hasSelection = false
		}
	}
	if a.layoutGesture.Active {
		if peer, exists := a.model.Clients[a.layoutGesture.ID]; !exists || !peer.State.Active {
			a.layoutGesture = layoutGesture{}
		}
	}
	if pending := a.layoutPending; pending != nil {
		if peer, exists := a.model.Clients[pending.ID]; !exists || !peer.State.Active {
			a.layoutPending = nil
		}
	}
}

// Confirm the originating peer's save even while another page/peer is selected.
func (a *application) syncLayout(c *ui.Context) {
	a.pruneLayoutState()
	pending := a.layoutPending
	if pending == nil {
		return
	}
	peer := a.model.Clients[pending.ID]
	actual := screenPlacement{peer.Config.Position, peer.Config.Alignment}
	switch {
	case actual == pending.Target, actual != pending.Before:
		a.layoutPending = nil
	case !c.Now().Before(pending.Until):
		a.layoutPending = nil
		a.model.Error = "Screen layout was not confirmed; try again"
		c.Invalidate()
	default:
		c.After(pending.Until.Sub(c.Now()))
	}
}

// Same-edge clients form a tangential group, never a fictitious daisy chain.
// Reserve one spare slot per edge so a drag target is always inside the fit.
func allScreenArrangement(width, height float32, clients []model.Client, moving uint64, pending *pendingPlacement) (ui.Rect, map[uint64]ui.Rect, map[string]ui.Rect) {
	if len(clients) == 1 {
		peer := clients[0]
		placement := screenPlacement{peer.Config.Position, peer.Config.Alignment}
		if pending != nil && pending.ID == peer.ID {
			placement = pending.Target
		}
		local, remote := screenArrangement(width, height, nil, nil, placement.Position, placement.Alignment)
		return local, map[uint64]ui.Rect{peer.ID: remote}, nil
	}
	counts := map[string]int{}
	for _, peer := range clients {
		counts[peer.Config.Position]++
	}
	columns := max(3, max(counts["top"], counts["bottom"])+1)
	band := max(1, max(counts["left"], counts["right"])+1)
	w := max(float32(1), min((width-32-float32(columns-1)*screenGap)/float32(columns), (height-layoutNameHeight-32-float32(band+1)*screenGap)/float32(band+2)*1.6))
	h := w / 1.6
	local := ui.Rect{X: (width - w) / 2, Y: (height - layoutNameHeight - h) / 2, W: w, H: h}
	groups := map[string][]uint64{}
	for _, peer := range clients {
		side := peer.Config.Position
		if pending != nil && pending.ID == peer.ID {
			side = pending.Target.Position
		}
		groups[side] = append(groups[side], peer.ID)
	}
	slot := func(side string, index, count int) ui.Rect {
		r := local
		if side == "left" || side == "right" {
			r.X += w + screenGap
			if side == "left" {
				r.X = local.X - w - screenGap
			}
			r.Y += (float32(index) - float32(count-1)/2) * (h + screenGap)
		} else {
			r.X += (float32(index) - float32(count-1)/2) * (w + screenGap)
			delta := max(float32(1), float32(max(counts["left"], counts["right"])+1)/2) * (h + screenGap)
			r.Y += delta
			if side == "top" {
				r.Y = local.Y - delta
			}
		}
		return r
	}
	rects, targets := map[uint64]ui.Rect{}, map[string]ui.Rect{}
	for _, side := range []string{"left", "right", "top", "bottom"} {
		ids := groups[side]
		for i, id := range ids {
			rects[id] = slot(side, i, len(ids))
		}
		index, count := len(ids), len(ids)+1
		for i, id := range ids {
			if id == moving {
				index, count = i, len(ids)
				break
			}
		}
		targets[side] = slot(side, index, count)
	}
	return local, rects, targets
}

func (a *application) otherActiveAt(peer model.Client, side string) (model.Client, bool) {
	for _, other := range a.model.SortedClients() {
		if other.ID != peer.ID && other.State.Active && other.Config.Position == side {
			return other, true
		}
	}
	return model.Client{}, false
}

func (a *application) layoutSideOccupied(peer model.Client, side string) (model.Client, bool) {
	if !peer.State.Active {
		return model.Client{}, false
	}
	return a.otherActiveAt(peer, side)
}

func pointInScreen(p ui.Point, r ui.Rect) bool {
	return p.X >= r.X && p.Y >= r.Y && p.X <= r.X+r.W && p.Y <= r.Y+r.H
}
