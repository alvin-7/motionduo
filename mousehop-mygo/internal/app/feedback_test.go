package app

import (
	"errors"
	"slices"
	"testing"
	"time"

	"github.com/alvin-7/mousehop/mygo/internal/model"
	"github.com/egoist/mygo/ui"
)

func TestApprovalPausedShowsExplicitRetryWithoutBackgroundWrites(t *testing.T) {
	a := fixture()
	a.page = "Devices"
	a.model.Pairing = nil
	peers := a.model.SortedClients()
	if len(peers) == 0 {
		t.Fatal("missing fixture peer")
	}
	peer := peers[0]
	peer.Config.ApprovalPaused = true
	peer.State.Alive = false
	a.model.Clients[peer.ID] = peer
	writes := 0
	a.request = func(kind string, value any) error {
		if kind != "RetryApproval" || value != peer.ID {
			t.Fatal(kind, value)
		}
		writes++
		return nil
	}
	tt := ui.NewTester(a.view, minWindowWidth, minWindowHeight)
	if !slices.Contains(tt.Texts(), "Request again") || !slices.Contains(tt.Texts(), "Approval required") {
		t.Fatal(tt.Texts())
	}
	tt.Advance(10 * time.Second)
	if writes != 0 {
		t.Fatal("background approval retry")
	}
	clickRelease(t, tt, "Request again")
	if writes != 1 {
		t.Fatal(writes)
	}
	peer.Config.ApprovalPaused = false
	a.model.Clients[peer.ID] = peer
	tt.Frame()
	if slices.Contains(tt.Texts(), "Request again") {
		t.Fatal("retry remained after canonical pending state")
	}
}

func emailTester(a *application) *ui.Tester {
	a.page, a.model.Pairing = "Settings", nil
	return ui.NewTester(a.view, minWindowWidth, minWindowHeight)
}

func TestEmailSaveWaitsForEchoAndHasNoIdleAnimation(t *testing.T) {
	a := fixture()
	writes := 0
	a.request = func(kind string, value any) error {
		writes++
		if kind != "SetDiscoveryEmail" || value != "new@example.com" {
			t.Fatal(kind, value)
		}
		return nil
	}
	tt := emailTester(a)
	clickRelease(t, tt, "Save email")
	if writes != 0 {
		t.Fatal("unchanged email was resubmitted")
	}
	a.emailDraft = " NEW@example.com "
	tt.Frame()
	clickRelease(t, tt, "Save email")
	clickRelease(t, tt, "Saving...")
	if writes != 1 || a.emailPending == nil || a.model.Email != "desk@example.com" || tt.HasText("Saved") {
		t.Fatal("pending save was duplicated or optimistically confirmed")
	}
	if _, err := a.receiveEvent([]byte(`{"DiscoveryEmail":"new@example.com"}`)); err != nil {
		t.Fatal(err)
	}
	tt.Frame()
	if a.emailPending != nil || a.emailDraft != "new@example.com" || !tt.HasText("Saved") {
		t.Fatal("canonical email acknowledgment did not update feedback", tt.Texts())
	}
	tt.Advance(4 * time.Second)
	if tt.HasText("Saved") || tt.FramePending() {
		t.Fatal("saved feedback did not settle without polling", tt.Texts())
	}
}

func TestEmailQueueFailureRejectionAndHiddenTimeoutKeepDraft(t *testing.T) {
	a := fixture()
	tt := emailTester(a)
	a.emailDraft = "new@example.com"
	a.request = func(string, any) error { return errors.New("queue full") }
	tt.Frame()
	clickRelease(t, tt, "Save email")
	if a.emailPending != nil || a.emailDraft != "new@example.com" || a.model.Error != "queue full" {
		t.Fatal("send failure discarded the draft")
	}
	a.request = func(string, any) error { return nil }
	clickRelease(t, tt, "Save email")
	if _, err := a.receiveEvent([]byte(`{"Error":"Enter a valid email address"}`)); err != nil {
		t.Fatal(err)
	}
	tt.Frame()
	if a.emailPending != nil || a.emailDraft != "new@example.com" || tt.HasText("Saved") {
		t.Fatal("backend rejection was treated as success")
	}
	clickRelease(t, tt, "Save email")
	a.page = "Devices"
	tt.Frame()
	tt.Advance(4 * time.Second)
	if a.emailPending != nil || a.emailDraft != "new@example.com" || !tt.HasText("Email was not confirmed") || tt.FramePending() {
		t.Fatal("hidden email timeout lost feedback", tt.Texts())
	}
}

func TestEmailCanonicalRefreshDoesNotOverwriteAnUnsubmittedDraft(t *testing.T) {
	a := fixture()
	tt := emailTester(a)
	a.model.Email = "external@example.com"
	tt.Frame()
	if a.emailDraft != a.model.Email {
		t.Fatal("clean editor did not follow canonical email")
	}
	a.emailDraft = "draft@example.com"
	a.model.Email = "changed-again@example.com"
	tt.Frame()
	if a.emailDraft != "draft@example.com" {
		t.Fatal("external update discarded an unsent draft")
	}
	a.model.Disconnect()
	tt.Frame()
	if a.emailPending != nil || a.model.EmailLoaded {
		t.Fatal("reconnection can edit email before loading fresh state")
	}
}

func TestDiscoveryConnectPreventsDuplicatesAndWaitsForCanonicalClient(t *testing.T) {
	a := fixture()
	a.model.Pairing = nil
	writes := 0
	a.request = func(string, any) error { writes++; return nil }
	tt := ui.NewTester(a.view, 680, 600)
	clickRelease(t, tt, "Connect")
	clickRelease(t, tt, "Connecting...")
	if writes != 1 || len(a.discoveryPending) != 1 {
		t.Fatal("Connect did not suppress a duplicate request")
	}
	peer := a.model.Clients[7]
	host := "studio.local"
	peer.ID, peer.Config.Hostname, peer.State.Alive = 8, &host, false
	a.model.Clients[8] = peer
	tt.Frame()
	if len(a.discoveryPending) != 0 || tt.HasText("Connecting...") || !tt.HasText("Waiting for connection") {
		t.Fatal("creation acknowledgment was mistaken for a live connection", tt.Texts())
	}
}

func TestDiscoveryRejectionAndTimeoutAllowRetry(t *testing.T) {
	a := fixture()
	a.model.Pairing = nil
	tt := ui.NewTester(a.view, 680, 600)
	clickRelease(t, tt, "Connect")
	if _, err := a.receiveEvent([]byte(`{"Error":"Device is no longer nearby"}`)); err != nil {
		t.Fatal(err)
	}
	tt.Frame()
	if len(a.discoveryPending) != 0 || !slices.Contains(tt.Texts(), "Connect") {
		t.Fatal("rejected request stayed disabled")
	}
	clickRelease(t, tt, "Connect")
	a.page = "Settings"
	tt.Frame()
	tt.Advance(4 * time.Second)
	if len(a.discoveryPending) != 0 || !tt.HasText("Connection request was not confirmed") || tt.FramePending() {
		t.Fatal("hidden Connect timeout did not settle", tt.Texts())
	}
}

func TestDeviceDisclosureAnimatesAndRespectsReducedMotion(t *testing.T) {
	a := fixture()
	a.model.Pairing, a.model.Incoming = nil, nil
	tt := ui.NewTester(a.view, 680, 1000)
	before := findScreen(t, tt, "Nearby")
	clickRelease(t, tt, "Configure MacBook Pro")
	tt.Advance(80 * time.Millisecond)
	during := findScreen(t, tt, "Nearby")
	tt.Advance(200 * time.Millisecond)
	after := findScreen(t, tt, "Nearby")
	if !(before.Y < during.Y && during.Y < after.Y) || tt.FramePending() {
		t.Fatal("configuration must reveal progressively, then stop animating", before, during, after)
	}
	tt.Key(0, ui.KeySpace)
	tt.Advance(250 * time.Millisecond)
	if a.configuredDevice != "" || tt.HasText("Reliable input (KCP)") {
		t.Fatal("keyboard disclosure did not collapse")
	}
	tt.SetPreferences(ui.Preferences{ReduceMotion: true})
	tt.Key(0, ui.KeyEnter)
	if a.configuredDevice != clientDeviceKey(7) || !tt.HasText("Trackpad momentum") || tt.FramePending() {
		t.Fatal("reduced-motion disclosure should settle at once", tt.Texts())
	}
}

func TestConnectionEscapeCancelsWithoutSaving(t *testing.T) {
	a := fixture()
	e := a.connectionEditor(7)
	e.Open = true
	writes := 0
	a.request = func(string, any) error { writes++; return nil }
	tt := connectionTester(a)
	clickRelease(t, tt, "Edit connection")
	clickRelease(t, tt, "Remote hostname")
	tt.Type("draft")
	tt.Key(0, ui.KeyEscape)
	if e.Editing || writes != 0 {
		t.Fatal("Escape did not cancel the unsent connection draft")
	}
}

func TestConnectionFailureUsesRealTransportStateWithoutExposingIdentity(t *testing.T) {
	a := fixture()
	a.model.Pairing = nil
	peer := a.model.Clients[7]
	peer.State.Alive, peer.State.Transport = false, "Failed: certificate hidden-certificate mismatch"
	a.model.Clients[7] = peer
	tt := ui.NewTester(a.view, 680, 600)
	if !tt.HasText("Device identity needs confirmation") || tt.HasText("hidden-certificate") {
		t.Fatal("transport failure is missing or reveals raw identity", tt.Texts())
	}
	peer.State.Transport = "Failed: connection timeout"
	a.model.Clients[7] = peer
	tt.Frame()
	if !tt.HasText("Connection timed out") {
		t.Fatal("actual backend failure was hidden behind a waiting label", tt.Texts())
	}
}

func TestNearbyRenamedHostDoesNotDuplicateSavedEndpoint(t *testing.T) {
	a := fixture()
	a.model.Pairing = nil
	peer := a.model.Clients[7]
	peer.Config.IPs = []string{"192.168.1.24"}
	a.model.Clients[7] = peer
	a.model.Nearby = []model.DiscoveredDevice{{ID: "renamed-instance", Name: "MacBook Pro (2)", Hostname: "macbook-pro-2.local", Port: peer.Config.Port, Addresses: []string{"192.168.1.24"}}}
	writes := 0
	a.request = func(string, any) error { writes++; return nil }
	tt := ui.NewTester(a.view, 680, 420)
	if slices.Contains(tt.Texts(), "Connect") || tt.HasText("MacBook Pro (2)") || !tt.HasText("Connected") || writes != 0 {
		t.Fatal("mDNS hostname change duplicated or modified a saved computer", tt.Texts(), writes)
	}
}

func TestDiscoveredEndpointMatchingDoesNotMergeNamesOrPorts(t *testing.T) {
	host, active := "saved.local.", "[2001:db8::5]:4252"
	peer := model.Client{Config: model.ClientConfig{Hostname: &host, Port: 4252, IPs: []string{"192.168.1.24"}}, State: model.ClientState{IPs: []string{"192.168.1.25"}, Address: &active}}
	for _, test := range []struct {
		name   string
		device model.DiscoveredDevice
		match  bool
	}{
		{"DNS case and final dot", model.DiscoveredDevice{Hostname: "SAVED.local", Port: 4252}, true},
		{"fixed IP after rename", model.DiscoveredDevice{Hostname: "saved-2.local", Port: 4252, Addresses: []string{"192.168.1.24"}}, true},
		{"resolved IP", model.DiscoveredDevice{Port: 4252, Addresses: []string{"192.168.1.25"}}, true},
		{"IPv6 active endpoint", model.DiscoveredDevice{Port: 4252, Addresses: []string{"2001:0db8:0:0:0:0:0:5"}}, true},
		{"same IP different service port", model.DiscoveredDevice{Hostname: "saved.local", Port: 4253, Addresses: []string{"192.168.1.24"}}, false},
		{"same display name different endpoint", model.DiscoveredDevice{Name: "saved.local.", Hostname: "another.local", Port: 4252, Addresses: []string{"192.168.1.26"}}, false},
		{"invalid addresses", model.DiscoveredDevice{Port: 4252, Addresses: []string{"not-an-ip", ""}}, false},
	} {
		t.Run(test.name, func(t *testing.T) {
			if got := matchesDiscovered(peer, test.device); got != test.match {
				t.Fatal("unexpected presentation match", got, test.match)
			}
		})
	}
}
