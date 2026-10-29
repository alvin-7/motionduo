package app

import (
	"encoding/json"
	"strings"
	"testing"

	"github.com/alvin-7/mousehop/mygo/internal/model"
)

func TestGlobalSettingsSnapshots(t *testing.T) {
	m := model.NewModel()
	if !m.GlobalKCP || !m.ClipboardSync {
		t.Fatal("global settings must default on")
	}
	for _, event := range []string{`{"GlobalKcp":false}`, `{"ClipboardSync":false}`} {
		if err := m.Apply([]byte(event)); err != nil {
			t.Fatal(err)
		}
	}
	if m.GlobalKCP || m.ClipboardSync {
		t.Fatal("global setting snapshots were not applied")
	}
}

func TestClipboardDeviceGrantsAreIndependent(t *testing.T) {
	m := model.NewModel()
	for _, event := range []string{`{"ClipboardDeviceGrants":{"fp-a":true}}`, `{"ClipboardDeviceGrants":{}}`} {
		if err := m.Apply([]byte(event)); err != nil {
			t.Fatal(err)
		}
	}
	if len(m.ClipboardGrants) != 0 {
		t.Fatal("snapshot retained revoked identity")
	}
	m.Disconnect()
	if m.ClipboardGrants != nil {
		t.Fatal("disconnect retained grants readiness")
	}
}

func TestLegacyDirectionalClipboardFieldsCannotGrantDeviceConsent(t *testing.T) {
	m := model.NewModel()
	for _, event := range []string{
		`{"Enumerate":[[3,{"expected_fingerprint":"fp-a","clipboard_send":true,"use_kcp":true},{"active":true}]]}`,
		`{"AuthorizedUpdated":{"fp-a":{"description":"Office","clipboard_receive":true,"natural_scroll":true}}}`,
	} {
		if err := m.Apply([]byte(event)); err != nil {
			t.Fatal(err)
		}
	}
	if len(m.ClipboardGrants) != 0 || !m.Clients[3].Config.KCP || !m.Incoming["fp-a"].NaturalScroll {
		t.Fatal("legacy fields granted consent or damaged input settings")
	}
}

func TestDaemonEventsAndHandleReuse(t *testing.T) {
	m := model.NewModel()
	for _, event := range []string{
		`{"Enumerate":[[3,{"hostname":"office.local","fix_ips":[],"port":4252,"pos":"right","alignment":"bottom_left"},{"active":true,"alive":true,"transport_status":"KCP","ips":[]}]]}`,
		`{"DiscoveryEmail":"User@example.com"}`,
		`{"AuthorizedUpdated":{"hidden-certificate":{"description":"Work laptop","mouse_sensitivity":1.5,"clipboard_receive":true}}}`,
		`{"DeviceConnected":{"fingerprint":"hidden-certificate","addr":"192.168.1.2:54321"}}`,
	} {
		if err := m.Apply([]byte(event)); err != nil {
			t.Fatal(err)
		}
	}
	if c := m.Clients[3]; c.Name() != "office.local" || !c.State.Alive || c.Config.Alignment != "bottom_left" {
		t.Fatalf("bad client: %#v", c)
	}
	if m.Incoming["hidden-certificate"].Sensitivity != 1.5 {
		t.Fatal("sensitivity decode")
	}
	if err := m.Apply([]byte(`{"Deleted":3}`)); err != nil {
		t.Fatal(err)
	}
	if err := m.Apply([]byte(`{"Created":[3,{"hostname":"new.local","pos":"left","port":4252},{"active":false}]}`)); err != nil {
		t.Fatal(err)
	}
	if m.Clients[3].State.Alive {
		t.Fatal("reused handle inherited live state")
	}
	m.Disconnect()
	if len(m.Connected) != 0 || m.Online || len(m.Nearby) != 0 || len(m.Pairing) != 0 {
		t.Fatal("disconnect retains live state")
	}
}

func TestUnknownEventsAndBadFrames(t *testing.T) {
	m := model.NewModel()
	if err := m.Apply([]byte(`{"FutureFeature":{"value":1}}`)); err != nil {
		t.Fatal(err)
	}
	for _, event := range []string{`{`, `{}`, `{"Created":[1,{}]}`, `{"Enumerate":[[1,{},"invalid"]]}`} {
		if err := m.Apply([]byte(event)); err == nil {
			t.Fatalf("accepted %s", event)
		}
	}
}

func TestRevokingIncomingAccessClearsOnlyThatPeersLiveState(t *testing.T) {
	m := model.NewModel()
	m.Connected = map[string]string{"revoked": "192.0.2.1:1", "retained": "192.0.2.2:2"}
	m.RemoteStates = map[string]string{"revoked": "Locked", "retained": "Locked", "outgoing": "Locked"}
	if err := m.Apply([]byte(`{"AuthorizedUpdated":{"retained":{"description":"Other computer"}}}`)); err != nil {
		t.Fatal(err)
	}
	if _, exists := m.Connected["revoked"]; exists {
		t.Fatal("revoked peer retained connected state")
	}
	if _, exists := m.RemoteStates["revoked"]; exists {
		t.Fatal("revoked peer retained host state")
	}
	if len(m.Connected) != 1 || len(m.RemoteStates) != 2 || m.RemoteStates["outgoing"] != "Locked" {
		t.Fatal("revocation changed another peer")
	}
}

func TestOnlyCompatibleIdentityEnablesMutations(t *testing.T) {
	a := newApplication()
	sent := false
	a.request = func(string, any) error { sent = true; return nil }
	a.send("SetDiscoveryEmail", "test@example.com")
	if sent {
		t.Fatal("mutated a daemon before protocol identification")
	}
	if err := a.model.Apply([]byte(`{"ServiceIdentity":{"pid":123,"version":"test","ipc_version":99}}`)); err != nil {
		t.Fatal(err)
	}
	if a.model.Online {
		t.Fatal("unsupported daemon became ready")
	}
	if err := a.model.Apply([]byte(`{"ServiceIdentity":{"pid":123,"version":"old-mygo","ipc_version":1}}`)); err != nil {
		t.Fatal(err)
	}
	if a.model.Online {
		t.Fatal("daemon without atomic drag placement became ready")
	}
	if err := a.model.Apply([]byte(`{"ServiceIdentity":{"pid":123,"version":"old-mygo","ipc_version":2}}`)); err != nil {
		t.Fatal(err)
	}
	if a.model.Online {
		t.Fatal("daemon without atomic connection editing became ready")
	}
	if err := a.model.Apply([]byte(`{"ServiceIdentity":{"pid":123,"version":"old-mygo","ipc_version":3}}`)); err != nil {
		t.Fatal(err)
	}
	if a.model.Online {
		t.Fatal("daemon without local identity and access revocation became ready")
	}
	if err := a.model.Apply([]byte(`{"ServiceIdentity":{"pid":123,"version":"old-mygo","ipc_version":5}}`)); err != nil {
		t.Fatal(err)
	}
	if a.model.Online {
		t.Fatal("daemon without aliases became ready")
	}
	if err := a.model.Apply([]byte(`{"ServiceIdentity":{"pid":123,"version":"test","ipc_version":8}}`)); err != nil {
		t.Fatal(err)
	}
	a.send("SetDiscoveryEmail", "test@example.com")
	if !sent {
		t.Fatal("compatible daemon rejected mutations")
	}
}

func TestDiscoveryDoesNotDecodeCertificateIntoPresentation(t *testing.T) {
	m := model.NewModel()
	err := m.Apply([]byte(`{"NearbyDevices":[{"id":"peer._mousehop._udp.local.","name":"Living room","hostname":"living-room.local","port":4252,"addresses":["192.168.1.3"],"fingerprint":"secret-fingerprint","platform":"macos"}]}`))
	if err != nil {
		t.Fatal(err)
	}
	data, _ := json.Marshal(m.Nearby)
	if strings.Contains(string(data), "secret-fingerprint") {
		t.Fatal("discovery presentation retains raw fingerprint")
	}
}
