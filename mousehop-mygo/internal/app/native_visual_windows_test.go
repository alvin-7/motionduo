package app

import (
	"log"
	"syscall"
)

func init() {
	nativeVisualProbe = func() {
		user := syscall.NewLazyDLL("user32.dll")
		context, _, _ := user.NewProc("GetThreadDpiAwarenessContext").Call()
		awareness, _, _ := user.NewProc("GetAwarenessFromDpiAwarenessContext").Call(context)
		log.Printf("native DPI awareness=%d (2=per-monitor)", awareness)
	}
}
