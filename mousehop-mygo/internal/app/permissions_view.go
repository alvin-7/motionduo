package app

import "github.com/egoist/mygo/ui"

func (a *application) permissionView(c *ui.Context) {
	p := a.permissions
	if p == nil || !p.Visible() {
		return
	}
	card(c.Key("input-permission"), tr("Input access"), func() {
		message := tr("Allow MotionDuo in macOS Accessibility.")
		kind, _ := p.Current.Missing()
		if kind == Monitoring {
			message = tr("Allow MotionDuo in macOS Input Monitoring.")
		} else if kind == Control {
			message = tr("Re-enable MotionDuo in macOS Accessibility.")
		}
		if p.RestartRequired {
			message = tr("Access updated. Restart MotionDuo to apply.")
		}
		if p.Err != nil {
			message = trf("Cannot check input access: %s", p.Err.Error())
		}
		ui.Text(c, message).TextColor(muted).FontSize(12)
		ui.Row(c).Gap(8).Children(func() {
			if p.Err == nil {
				if p.RestartRequired {
					if ui.PrimaryButton(c, tr("Restart MotionDuo")).Clicked() && a.restart != nil {
						a.restart()
					}
				} else if ui.PrimaryButton(c, tr("Open settings")).Clicked() {
					p.OpenSettings()
				}
			}
			if ui.Button(c, tr("Check again")).Clicked() {
				p.Refresh()
			}
		})
	})
}
