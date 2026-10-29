package app

import (
	"math"

	"github.com/egoist/mygo/ui"
)

func drawDisclosureChevron(p *ui.Painter, r ui.Rect, progress float32, color ui.Color) {
	angle := float64(progress) * math.Pi / 2
	cos, sin := float32(math.Cos(angle)), float32(math.Sin(angle))
	cx, cy := r.X+r.W/2, r.Y+r.H/2
	point := func(x, y float32) (float32, float32) {
		return cx + x*cos - y*sin, cy + x*sin + y*cos
	}
	x1, y1 := point(-2, -3)
	x2, y2 := point(1, 0)
	x3, y3 := point(-2, 3)
	// Painter.Line paints axis-aligned rules; diagonals need a stroked path.
	path := new(ui.Path).MoveTo(x1, y1).LineTo(x2, y2).LineTo(x3, y3)
	p.StrokePath(path, 1.5, color)
}
