package app

import "github.com/egoist/mygo/ui"

// Keep the toolkit's keyboard, focus, disabled and pointer handling intact.
func quietButton(c *ui.Context, label string) ui.Element {
	return ui.Button(c, label).Radius(7).Border(0.5, ui.Hex("#48474e")).
		InsetShadow(0, 0.5, 0, 0, ui.RGBA(255, 255, 255, 0.065)).
		Shadow(0, 1, 2, 0, ui.RGBA(0, 0, 0, 0.16))
}

func accentButton(c *ui.Context, label string) ui.Element {
	return ui.PrimaryButton(c, label).Radius(7).
		InsetShadow(0, 0.5, 0, 0, ui.RGBA(255, 255, 255, 0.28)).
		Shadow(0, 1, 3, 0, ui.RGBA(0, 0, 0, 0.2))
}
