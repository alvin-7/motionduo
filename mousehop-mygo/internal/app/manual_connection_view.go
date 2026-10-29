package app

import (
	"math"
	"strings"

	"github.com/egoist/mygo/ui"
)

func (a *application) manualConnectionView(c *ui.Context) {
	ui.Modal(c.Key("manual-connection"), &a.manualOpen, func() {
		ui.Column(c).Width(320).Gap(12).Children(func() {
			ui.Text(c, tr("Connect by address")).Bold()
			ui.TextInput(c, &a.manualHost).Label(tr("Computer address")).Placeholder(tr("Hostname or IP address"))
			ui.NumberInput(c, &a.manualPort, 1, 65535, 1).Label(tr("Remote port"))
			ui.Row(c).Gap(8).Children(func() {
				ui.Spacer(c)
				if quietButton(c, tr("Cancel")).Clicked() {
					a.manualOpen = false
				}
				valid := strings.TrimSpace(a.manualHost) != "" && a.manualPort >= 1 && a.manualPort <= 65535 && a.manualPort == math.Trunc(a.manualPort)
				if accentButton(c, tr("Add computer")).Disabled(!a.model.Online || !valid).Clicked() {
					if a.send("ConnectManual", []any{strings.TrimSpace(a.manualHost), int(a.manualPort), "right"}) {
						a.manualOpen = false
					}
				}
			})
		})
	})
}
