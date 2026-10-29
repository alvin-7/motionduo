//go:build darwin

package app

import (
	"fmt"
	"unsafe"

	"github.com/ebitengine/purego"
	"github.com/egoist/mygo"
)

type macPermissionAPI struct {
	accessibility  func() uint8
	monitoring     func() uint8
	control        func() uint8
	requestAX      func(uintptr) uint8
	requestListen  func() uint8
	requestPost    func() uint8
	dictionary     func(uintptr, *uintptr, *uintptr, int64, uintptr, uintptr) uintptr
	release        func(uintptr)
	promptKey      uintptr
	trueValue      uintptr
	keyCallbacks   uintptr
	valueCallbacks uintptr
	err            error
}

func newPlatformPermissions() *Controller {
	return &Controller{Backend: loadMacPermissions()}
}

func loadMacPermissions() *macPermissionAPI {
	a := &macPermissionAPI{}
	var handles []uintptr
	open := func(name string) uintptr {
		if a.err != nil {
			return 0
		}
		h, err := purego.Dlopen("/System/Library/Frameworks/"+name+".framework/"+name, purego.RTLD_LAZY|purego.RTLD_LOCAL)
		if err != nil {
			a.err = err
			return 0
		}
		handles = append(handles, h)
		return h
	}
	ax, cg, cf := open("ApplicationServices"), open("CoreGraphics"), open("CoreFoundation")
	symbol := func(lib uintptr, name string) uintptr {
		if a.err != nil {
			return 0
		}
		address, err := purego.Dlsym(lib, name)
		if err != nil || address == 0 {
			a.err = fmt.Errorf("macOS permission API %s unavailable: %v", name, err)
		}
		return address
	}
	bind := func(fn any, lib uintptr, name string) {
		if address := symbol(lib, name); address != 0 {
			purego.RegisterFunc(fn, address)
		}
	}
	bind(&a.accessibility, ax, "AXIsProcessTrusted")
	bind(&a.requestAX, ax, "AXIsProcessTrustedWithOptions")
	bind(&a.monitoring, cg, "CGPreflightListenEventAccess")
	bind(&a.control, cg, "CGPreflightPostEventAccess")
	bind(&a.requestListen, cg, "CGRequestListenEventAccess")
	bind(&a.requestPost, cg, "CGRequestPostEventAccess")
	bind(&a.dictionary, cf, "CFDictionaryCreate")
	bind(&a.release, cf, "CFRelease")
	// These addresses name static native pointer globals, not Go heap objects.
	// Match MyGo's CoreFoundation binding: reinterpret the address word first.
	if address := symbol(ax, "kAXTrustedCheckOptionPrompt"); address != 0 {
		a.promptKey = **(**uintptr)(unsafe.Pointer(&address))
	}
	if address := symbol(cf, "kCFBooleanTrue"); address != 0 {
		a.trueValue = **(**uintptr)(unsafe.Pointer(&address))
	}
	// The callbacks are exported structs; unlike CFStringRef/CFBooleanRef globals,
	// use their addresses directly rather than dereferencing their first word.
	a.keyCallbacks = symbol(cf, "kCFTypeDictionaryKeyCallBacks")
	a.valueCallbacks = symbol(cf, "kCFTypeDictionaryValueCallBacks")
	if a.err != nil {
		for _, handle := range handles {
			_ = purego.Dlclose(handle)
		}
	}
	// Successful framework handles deliberately live as long as these functions.
	return a
}

func (a *macPermissionAPI) Snapshot() (State, error) {
	if a.err != nil {
		return State{}, a.err
	}
	return Read(a.accessibility, a.monitoring, a.control), nil
}

func (a *macPermissionAPI) Request(kind Permission) error {
	if a.err != nil {
		return a.err
	}
	switch kind {
	case Accessibility:
		dict := a.dictionary(0, &a.promptKey, &a.trueValue, 1, a.keyCallbacks, a.valueCallbacks)
		if dict == 0 {
			return fmt.Errorf("cannot create macOS permission options")
		}
		defer a.release(dict)
		a.requestAX(dict)
	case Monitoring:
		a.requestListen()
	case Control:
		a.requestPost()
	}
	// The OS grant is asynchronous; do not treat this call's return as approval.
	return nil
}

func (a *macPermissionAPI) OpenSettings(kind Permission) error {
	return mygo.Shell.OpenExternal(Pane(kind))
}
