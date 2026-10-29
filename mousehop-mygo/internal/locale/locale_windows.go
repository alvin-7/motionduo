package locale

import (
	"syscall"
	"unsafe"
)

func SystemLocale() string {
	// Read the user's Windows UI language, never shell LANG/LC_* overrides.
	language, _, _ := syscall.NewLazyDLL("kernel32.dll").NewProc("GetUserDefaultUILanguage").Call()
	var name [85]uint16
	n, _, _ := syscall.NewLazyDLL("kernel32.dll").NewProc("LCIDToLocaleName").Call(language, uintptr(unsafe.Pointer(&name[0])), uintptr(len(name)), 0)
	if n == 0 {
		return ""
	}
	return syscall.UTF16ToString(name[:])
}
