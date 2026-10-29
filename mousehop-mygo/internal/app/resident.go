package app

import "github.com/egoist/mygo"

type residentWindow interface {
	Show()
	Hide()
	Focus()
	IsMinimized() bool
	Restore()
}

func openResidentWindow(win residentWindow) {
	if win.IsMinimized() {
		win.Restore()
	}
	win.Show()
	win.Focus()
}

func parkResidentWindow(win residentWindow) {
	win.Hide()
}

func residentActivationPolicy(platform string) mygo.ActivationPolicy {
	if platform == "darwin" {
		return mygo.ActivationPolicyAccessory
	}
	return mygo.ActivationPolicyRegular
}

type loginSettings interface {
	OpenAtLogin() bool
	SetOpenAtLogin(bool) error
}

type residentMenu struct {
	menu  *mygo.Menu
	login loginSettings
}

func newResidentMenu(login loginSettings, open, restart func(), report func(error)) *residentMenu {
	m := &residentMenu{login: login}
	m.menu = mygo.NewMenu([]*mygo.MenuItem{
		{ID: "open", Label: tr("Open MotionDuo"), Click: func(*mygo.MenuItem, *mygo.Window) { open() }},
		{ID: "open-at-login", Label: tr("Open at login"), Type: mygo.MenuItemCheckbox, Checked: login.OpenAtLogin(), Click: func(item *mygo.MenuItem, _ *mygo.Window) {
			if err := login.SetOpenAtLogin(item.Checked); err != nil {
				report(err)
			}
			// Read the OS result, including disabled or approval-pending items.
			m.refresh()
		}},
		mygo.Separator(),
		{ID: "restart", Label: tr("Restart MotionDuo"), Click: func(*mygo.MenuItem, *mygo.Window) { restart() }},
		{ID: "quit", Label: tr("Quit"), Role: mygo.RoleQuit},
	})
	return m
}

func (m *residentMenu) refresh() {
	m.menu.ItemByID("open-at-login").SetChecked(m.login.OpenAtLogin())
}
