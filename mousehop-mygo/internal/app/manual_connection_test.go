package app

import (
	"reflect"
	"testing"

	"github.com/egoist/mygo/ui"
)

func TestManualConnectionDialog(t *testing.T) {
	a := fixture()
	a.model.Pairing = nil
	var kind string
	var value any
	a.request = func(k string, v any) error { kind, value = k, v; return nil }
	tt := ui.NewTester(a.view, minWindowWidth, minWindowHeight)
	clickRelease(t, tt, "Connect by address")
	if !a.manualOpen {
		t.Fatal("dialog did not open")
	}
	clickRelease(t, tt, "Cancel")
	if a.manualOpen || kind != "" {
		t.Fatal("cancel sent a request")
	}
	clickRelease(t, tt, "Connect by address")
	a.manualHost, a.manualPort = "  example.local  ", 4252
	tt = ui.NewTester(a.view, minWindowWidth, minWindowHeight)
	clickRelease(t, tt, "Add computer")
	if a.manualOpen || kind != "ConnectManual" || !reflect.DeepEqual(value, []any{"example.local", 4252, "right"}) {
		t.Fatal(kind, value, a.manualOpen)
	}
}
