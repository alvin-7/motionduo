package app

import (
	"encoding/json"
	"errors"
	"math"
	"strings"
	"testing"
	"time"

	"github.com/egoist/mygo/ui"
)

func TestConnectionDraftValidation(t *testing.T) {
	d := connectionDraft{Hostname: " office.local ", IPs: "192.0.2.7; 2001:0db8::7,192.0.2.7", Port: 4253}
	s, err := d.settings()
	if err != nil || optionalText(s.Hostname) != "office.local" || strings.Join(s.IPs, ",") != "192.0.2.7,2001:db8::7" {
		t.Fatal(s, err)
	}
	for _, bad := range []connectionDraft{
		{Port: 4252}, {Hostname: "space here", Port: 4252},
		{Hostname: "pc", Port: 0}, {Hostname: "pc", Port: math.NaN()},
		{Hostname: "pc", Port: 1.5}, {Hostname: "pc", Port: 65536},
		{Hostname: "pc", Port: 4252, IPs: "not-an-ip"},
		{IPs: "fe80::1%en0", Port: 4252}, {Hostname: "pc", Port: 4252, Hook: "echo\x00"},
	} {
		if _, err := bad.settings(); err == nil {
			t.Fatalf("accepted invalid connection: %+v", bad)
		}
	}
}

func connectionTester(a *application) *ui.Tester {
	return ui.NewTester(func(c *ui.Context) {
		theme := graphiteTheme()
		c.SetTheme(&theme)
		ui.Column(c).Padding(14).Children(func() { a.connectionView(c, a.model.Clients[7]) })
	}, 540, 600)
}

func TestAdvancedConnectionSavesAtomicallyAndWaitsForEcho(t *testing.T) {
	a := fixture()
	e := a.connectionEditor(7)
	e.Open = true
	var calls []string
	a.request = func(kind string, value any) error {
		data, _ := json.Marshal(map[string]any{kind: value})
		calls = append(calls, string(data))
		return nil
	}
	tt := connectionTester(a)
	clickRelease(t, tt, "Edit connection")
	clickRelease(t, tt, "Remote hostname")
	tt.Key(ui.Cmd, ui.KeyA)
	tt.Type("office.local")
	if len(calls) != 0 {
		t.Fatal("typing wrote connection settings", calls)
	}
	clickRelease(t, tt, "Save connection")
	if len(calls) != 1 || calls[0] != `{"UpdateConnection":[7,{"hostname":"office.local","fix_ips":[],"port":4252,"cmd":null}]}` || e.Pending == nil {
		t.Fatal(calls, e.Pending)
	}
	if optionalText(a.model.Clients[7].Config.Hostname) == "office.local" {
		t.Fatal("save was optimistically treated as confirmed")
	}
	peer := a.model.Clients[7]
	peer.Config.Hostname = e.Pending.Target.Hostname
	a.model.Clients[7] = peer
	tt.Frame()
	if e.Pending != nil {
		t.Fatal("canonical state did not acknowledge the save")
	}
	clickRelease(t, tt, "Edit connection")
	e.Draft.Port++
	tt.Frame()
	a.request = func(string, any) error { return errors.New("queue full") }
	clickRelease(t, tt, "Save connection")
	if !e.Editing || e.Pending != nil || a.model.Error != "queue full" {
		t.Fatal("queue failure discarded the draft")
	}
	clickRelease(t, tt, "Cancel connection")
	if e.Editing {
		t.Fatal("cancel kept the editor open")
	}
}

func TestNetworkChoicesUseStableIDsAndKeepOfflinePin(t *testing.T) {
	peer := fixture().model.Clients[7]
	fast, zero, pin := uint32(1520), uint32(0), "192.0.2.9"
	peer.State.IPs = []string{"192.0.2.1", "192.0.2.2", "192.0.2.3"}
	peer.State.ActiveLock = &pin
	peer.State.Latencies = map[string]*uint32{"192.0.2.1": &fast, "192.0.2.2": nil, "192.0.2.3": &zero}
	choices := networkChoices(peer)
	if networkSelection(peer) != "ip:192.0.2.9" || len(choices) != 6 {
		t.Fatal(choices)
	}
	for i, suffix := range []string{"1.5 ms", "Unreachable", "<0.1 ms", "Not measured"} {
		if !strings.HasSuffix(choices[i+2].Label, suffix) {
			t.Fatal(choices)
		}
	}
	fast = 2500
	updated := networkChoices(peer)
	for i := range choices {
		if updated[i].ID != choices[i].ID {
			t.Fatal("RTT changed selection identity")
		}
	}
}

func TestNetworkSelectionWaitsForCanonicalStateAndHiddenDeadline(t *testing.T) {
	a := fixture()
	e := a.connectionEditor(7)
	e.Open = true
	var request string
	a.request = func(kind string, value any) error {
		data, _ := json.Marshal(map[string]any{kind: value})
		request = string(data)
		return nil
	}
	tt := connectionTester(a)
	clickRelease(t, tt, "Network path")
	clickRelease(t, tt, "auto")
	if request != `{"SetConnectionMode":[7,"auto"]}` || e.Network == nil || networkSelection(a.model.Clients[7]) != "fastest" {
		t.Fatal("network choice did not await canonical echo", request)
	}
	a.page, a.model.Pairing = "Devices", nil
	// No connection controls are mounted, but the save deadline is still serviced.
	tt = ui.NewTester(a.view, minWindowWidth, minWindowHeight)
	e.Network.Until = tt.Now().Add(time.Second)
	tt.Frame()
	tt.Advance(2 * time.Second)
	if e.Network != nil || !tt.HasText("Network choice was not confirmed") || tt.FramePending() {
		t.Fatal("hidden network save kept pending", tt.Texts())
	}
	delete(a.model.Clients, 7)
	a.pruneConnectionEditors()
	if len(a.connectionEditors) != 0 {
		t.Fatal("deleted/reused handle can inherit old drafts")
	}
}
