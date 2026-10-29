package app

import "github.com/egoist/mygo/ui"

// Ballmac's recessed track and raised selection, using native radio semantics.
func (a *application) navigation(c *ui.Context) {
	labels := []string{"Devices", "Screen layout", "Settings", "About"}
	selected := 0
	for i, label := range labels {
		if a.page == label {
			selected = i
		}
	}
	parts := ui.SegmentedBase(c.Key("navigation"), &selected, len(labels))
	parts.Track.Label(tr("Pages")).Height(32).Padding(2).Gap(1).Radius(9).
		Background(ui.Hex("#303034")).Border(0.5, ui.Hex("#414146")).
		InsetShadow(0, 1, 2, 0, ui.RGBA(0, 0, 0, 0.12)).Children(func() {
		for i, label := range labels {
			segment := parts.Segment(i).Height(27).Padding(0, 10).Radius(7).Label(tr(label))
			color := muted
			if segment.Hovered() {
				color = textPrimary
				segment.Background(ui.Hex("#39393e"))
			}
			if i == selected {
				color = textPrimary
				segment.Background(ui.Hex("#505056")).Border(0.5, ui.Hex("#202023")).
					InsetShadow(0, 0.5, 0, 0, ui.RGBA(255, 255, 255, 0.12)).
					Shadow(0, 1, 2, 0, ui.RGBA(0, 0, 0, 0.3))
			}
			segment.Children(func() { ui.Text(c, tr(label)).FontSize(13).TextColor(color).SingleLine() })
		}
	})
	if parts.Track.Changed() {
		a.page = labels[selected]
	}
}
