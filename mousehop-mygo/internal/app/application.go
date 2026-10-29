package app

import (
	"context"
	"fmt"
	"log"
	"runtime"
	"sync"
	"time"

	"github.com/alvin-7/mousehop/mygo/internal/ipc"
	"github.com/alvin-7/mousehop/mygo/internal/locale"
	"github.com/alvin-7/mousehop/mygo/internal/model"
	"github.com/egoist/mygo"
	"github.com/egoist/mygo/ui"
)

const (
	windowWidth     = 680
	windowHeight    = 420
	minWindowWidth  = 600
	minWindowHeight = 400
)

type application struct {
	mu                 sync.Mutex
	model              model.Model
	ipc                *ipc.Client
	win                *mygo.Window
	page               string
	advancedOpen       bool
	configuredDevice   string
	permissions        *Controller
	fullDisk           func() bool
	fullDiskGranted    bool
	restart            func()
	emailDraft         string
	emailInitialized   bool
	emailObserved      string
	emailPending       *pendingEmail
	emailSavedUntil    time.Time
	discoveryPending   map[string]pendingDiscovery
	selected           uint64
	hasSelection       bool
	manualOpen         bool
	manualHost         string
	manualPort         float64
	suppressedDraft    string
	newPort            float64
	portInitialized    bool
	releaseEditor      ShortcutEditor
	releasePending     *PendingShortcut
	thresholdEditor    ThresholdEditor
	connectionEditors  map[uint64]*connectionEditor
	quitting           bool
	layoutCanvas       ui.Rect
	layoutGesture      layoutGesture
	layoutPending      *pendingPlacement
	request            func(string, any) error
	compatibilityTimer *time.Timer
	serviceGeneration  uint64
}

func newApplication() *application {
	return &application{model: model.NewModel(), ipc: ipc.New(), page: "Devices", manualPort: 4252, fullDisk: fullDiskAccess, fullDiskGranted: true}
}

func (a *application) refreshFullDiskAccess() {
	if a.fullDisk == nil {
		a.fullDisk = fullDiskAccess
	}
	a.fullDiskGranted = a.fullDisk()
}

func (a *application) send(kind string, value any) bool {
	if !a.model.Online {
		a.model.Error = "Waiting for a compatible daemon"
		return false
	}
	var err error
	if a.request != nil {
		err = a.request(kind, value)
	} else {
		err = a.ipc.Send(kind, value)
	}
	if err != nil {
		a.model.Error = err.Error()
		return false
	}
	return true
}

// reportError shows a message from a background goroutine.
func (a *application) reportError(message string) {
	a.mu.Lock()
	a.model.Error = message
	a.mu.Unlock()
	if a.win != nil {
		a.win.Invalidate()
	}
}

func (a *application) show() { openResidentWindow(a.win) }

func (a *application) ready(ctx context.Context) {
	locale.Language = locale.LanguageForLocale(locale.SystemLocale())
	mygo.Theme.SetSource(mygo.ThemeDark)
	a.restart = mygo.App.Relaunch
	a.permissions = newPlatformPermissions()
	if a.permissions != nil {
		a.permissions.Refresh()
	}
	a.refreshFullDiskAccess()
	openedAtLogin := mygo.App.WasOpenedAtLogin()
	a.win = mygo.NewWindow(mygo.WindowOptions{Title: "MotionDuo", Width: windowWidth, Height: windowHeight, MinWidth: minWindowWidth, MinHeight: minWindowHeight, Hidden: openedAtLogin, TitleBarStyle: mygo.TitleBarHidden, TitleBarHeight: 36, Content: ui.View(a.view)})
	if err := a.win.SetIcon(applicationIcon); err != nil {
		log.Printf("window icon unavailable: %v", err)
	}
	mygo.App.OnActivate(func(bool) { a.show() })
	mygo.App.OnSecondInstance(func([]string, string) { a.show() })
	hideOnClose := true
	a.win.OnClose(func(e *mygo.CloseEvent) {
		if !a.quitting && hideOnClose {
			e.PreventDefault()
			parkResidentWindow(a.win)
		}
	})
	mygo.App.OnBeforeQuit(func(*mygo.QuitEvent) { a.quitting = true })
	menu := newResidentMenu(mygo.App, a.show, mygo.App.Relaunch, func(err error) {
		a.mu.Lock()
		a.model.Error = fmt.Sprintf("Open at login: %v", err)
		a.mu.Unlock()
		a.show()
		a.win.Invalidate()
	})
	mygo.App.OnDidBecomeActive(menu.refresh)
	if a.permissions != nil {
		mygo.App.OnDidBecomeActive(func() {
			a.mu.Lock()
			a.permissions.Refresh()
			a.refreshFullDiskAccess()
			a.mu.Unlock()
			a.win.Invalidate()
		})
	} else {
		mygo.App.OnDidBecomeActive(func() {
			a.mu.Lock()
			a.refreshFullDiskAccess()
			a.mu.Unlock()
			a.win.Invalidate()
		})
	}
	tray, err := mygo.NewTray(mygo.TrayOptions{ID: "mousehop-resident", Icon: trayIconForPlatform(runtime.GOOS), IconIsTemplate: runtime.GOOS == "darwin", ToolTip: "MotionDuo"})
	if err != nil {
		// Without a tray, closing must remain a usable way to quit.
		hideOnClose = false
		log.Printf("tray unavailable: %v", err)
		if openedAtLogin {
			a.show()
		}
	} else {
		if runtime.GOOS == "linux" {
			tray.SetMenu(menu.menu)
		} else {
			popup := func() {
				menu.refresh()
				tray.SetMenu(menu.menu)
				defer tray.SetMenu(nil)
				tray.PopUpMenu()
			}
			if runtime.GOOS == "darwin" {
				tray.OnClick(popup)
			} else {
				tray.OnClick(a.show)
			}
			tray.OnRightClick(popup)
		}
		mygo.App.OnWillQuit(func(*mygo.QuitEvent) { tray.Destroy() })
		if openedAtLogin {
			parkResidentWindow(a.win)
		}
	}
	a.ipc.OnEvent = func(data []byte) error {
		show, err := a.receiveEvent(data)
		a.win.Invalidate()
		if show {
			a.show()
		}
		return err
	}
	a.ipc.OnStatus = func(online bool, err error) {
		a.mu.Lock()
		a.serviceGeneration++
		generation := a.serviceGeneration
		if a.compatibilityTimer != nil {
			a.compatibilityTimer.Stop()
		}
		if online {
			a.model.Online, a.model.ServiceConnected = false, true
			a.compatibilityTimer = time.AfterFunc(2*time.Second, func() {
				a.mu.Lock()
				if generation == a.serviceGeneration && a.model.ServiceConnected && !a.model.Online {
					a.model.Error = model.IncompatibleDaemon
				}
				a.mu.Unlock()
				a.win.Invalidate()
			})
		} else {
			a.model.Disconnect()
			a.releaseEditor.Open, a.releasePending = false, nil
			a.thresholdEditor = ThresholdEditor{}
			a.connectionEditors = nil
			a.configuredDevice = ""
		}
		a.mu.Unlock()
		a.win.Invalidate()
	}
	go a.ipc.Run(ctx)
}

func (a *application) receiveEvent(data []byte) (bool, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	previous := make(map[string]bool, len(a.model.Pairing))
	for _, request := range a.model.Pairing {
		previous[request.ID] = true
	}
	if err := a.model.Apply(data); err != nil {
		return false, err
	}
	a.pruneConnectionEditors()
	a.pruneDeviceSettings()
	a.pruneLayoutState()
	for _, request := range a.model.Pairing {
		if !previous[request.ID] {
			return true, nil
		}
	}
	return false, nil
}
