package app

import "github.com/egoist/mygo/ui"

func layoutMonitor(c *ui.Context, r ui.Rect, name string, remote bool, cursor ui.Cursor, enabled bool) {
	label := tr("Local screen")
	if remote {
		label = tr("Remote screen")
	}
	layoutMonitorState(c, r, name, label, remote, cursor, enabled)
}

// Decoration and names stay inside the authoritative 16:10 hit rectangle.
func layoutMonitorState(c *ui.Context, r ui.Rect, name, label string, selected bool, cursor ui.Cursor, active bool) {
	edge, face := ui.Hex("#65656d"), ui.Hex("#303035")
	if selected {
		edge, face = blue, ui.Hex("#303743")
	}
	opacity := float32(1)
	if !active {
		opacity = 0.55
	}
	ui.Column(c).Label(label).Absolute().Left(r.X).Top(r.Y).Size(r.W, r.H).
		Center().Padding(6).Radius(7).Border(1, edge).Gradient(face, ui.Hex("#242428"), 90).
		InsetShadow(0, 1, 0, 0, ui.RGBA(255, 255, 255, 0.13)).
		Shadow(0, 5, 12, -3, ui.RGBA(0, 0, 0, 0.42)).Cursor(cursor).Opacity(opacity).
		Draw(func(p *ui.Painter, box ui.Rect) {
			p.Stroke(ui.Rect{X: box.X + 4, Y: box.Y + 4, W: box.W - 8, H: box.H - 13}, ui.RGBA(255, 255, 255, 0.055), 3, 0.5)
			p.Line(box.X+box.W/2-7, box.Y+box.H-5, box.X+box.W/2+7, box.Y+box.H-5, 1, edge)
		}).Children(func() {
		ui.Text(c.Key("name"), name).Width(max(float32(1), r.W-16)).FontSize(min(float32(13), max(float32(8), r.H/5))).Bold().TextColor(textPrimary).MaxLines(1).Ellipsis("…").Tooltip(name)
		if !active {
			ui.Text(c.Key("inactive"), tr("Inactive")).FontSize(min(float32(10), max(float32(6), r.H/6))).TextColor(muted).MaxLines(1)
		}
	})
}
