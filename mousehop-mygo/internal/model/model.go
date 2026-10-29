package model

import (
	"encoding/json"
	"fmt"
	"sort"
)

const IncompatibleDaemon = "Incompatible daemon. Quit the other MotionDuo instance, then reopen this build."
const FrontendIPCVersion = 8

type ClientConfig struct {
	ExpectedFingerprint *string  `json:"expected_fingerprint"`
	Alias               string   `json:"alias"`
	ApprovalPaused      bool     `json:"approval_paused"`
	ApprovalNonce       uint64   `json:"approval_nonce"`
	Hostname            *string  `json:"hostname"`
	IPs                 []string `json:"fix_ips"`
	Port                int      `json:"port"`
	Position            string   `json:"pos"`
	Alignment           string   `json:"alignment"`
	CommandAsCtrl       bool     `json:"command_as_ctrl"`
	KCP                 bool     `json:"use_kcp"`
	ScrollInertia       bool     `json:"scroll_inertia"`
	RequireModifier     bool     `json:"require_crossing_modifier"`
	Modifier            string   `json:"crossing_modifier"`
	Mode                string   `json:"mode"`
	Hook                *string  `json:"cmd"`
}

type ClientState struct {
	DisplaySize *[2]uint32         `json:"display_size"`
	Active      bool               `json:"active"`
	Alive       bool               `json:"alive"`
	Address     *string            `json:"active_addr"`
	IPs         []string           `json:"ips"`
	Transport   string             `json:"transport_status"`
	LastError   *string            `json:"last_error"`
	Interfaces  map[string]string  `json:"interfaces"`
	Latencies   map[string]*uint32 `json:"latencies"`
	ActiveLock  *string            `json:"active_lock"`
	Resolving   bool               `json:"resolving"`
}

type Client struct {
	ID     uint64
	Config ClientConfig
	State  ClientState
}

func (c Client) Name() string {
	if c.Config.Alias != "" {
		return c.Config.Alias
	}
	return c.IdentityName()
}

func (c Client) IdentityName() string {
	if c.Config.Hostname != nil && *c.Config.Hostname != "" {
		return *c.Config.Hostname
	}
	if len(c.Config.IPs) > 0 {
		return c.Config.IPs[0]
	}
	return fmt.Sprintf("Device %d", c.ID+1)
}

type DiscoveredDevice struct {
	ID        string   `json:"id"`
	Name      string   `json:"name"`
	Hostname  string   `json:"hostname"`
	Addresses []string `json:"addresses"`
	Port      int      `json:"port"`
	Platform  string   `json:"platform"`
}

type PairingRequest struct {
	ID        string   `json:"id"`
	Name      string   `json:"name"`
	Addresses []string `json:"addresses"`
}

type IncomingPeer struct {
	Description   string  `json:"description"`
	NaturalScroll bool    `json:"natural_scroll"`
	Sensitivity   float64 `json:"mouse_sensitivity"`
}

type RunningApp struct {
	Name       string `json:"display_name"`
	Identifier string `json:"identifier"`
}

type Model struct {
	DeviceName        string
	LocalDisplaySize  *[2]uint32
	Online            bool
	ServiceConnected  bool
	Email             string
	EmailLoaded       bool
	Clients           map[uint64]Client
	Nearby            []DiscoveredDevice
	Pairing           []PairingRequest
	Incoming          map[string]IncomingPeer
	Connected         map[string]string
	RemoteStates      map[string]string
	ClipboardGrants   map[string]bool
	GlobalKCP         bool
	ClipboardSync     bool
	Suppressed        []string
	Running           []RunningApp
	Capture           string
	Emulation         string
	MDNS              bool
	Port              int
	ReleaseThreshold  uint32
	ReleaseBind       []string
	ThresholdLoaded   bool
	ReleaseBindLoaded bool
	Error             string
	ErrorSequence     uint64
	Version           string
}

func NewModel() Model {
	return Model{DeviceName: "This computer", Clients: map[uint64]Client{}, Incoming: map[string]IncomingPeer{}, Connected: map[string]string{}, RemoteStates: map[string]string{}, Port: 4252, MDNS: true, GlobalKCP: true, ClipboardSync: true}
}

func (m *Model) Disconnect() {
	m.ClipboardGrants = nil
	m.Online = false
	m.ServiceConnected = false
	m.EmailLoaded = false
	m.ThresholdLoaded, m.ReleaseBindLoaded = false, false
	m.LocalDisplaySize = nil
	m.Nearby = nil
	m.Pairing = nil
	m.Connected = map[string]string{}
	for id, c := range m.Clients {
		c.State.Alive = false
		c.State.DisplaySize = nil
		c.State.Address, c.State.ActiveLock = nil, nil
		c.State.Latencies, c.State.Interfaces = nil, nil
		c.State.Resolving = false
		m.Clients[id] = c
	}
}

func DecodeClient(raw json.RawMessage) (Client, error) {
	var tuple []json.RawMessage
	var c Client
	if err := json.Unmarshal(raw, &tuple); err != nil {
		return c, err
	}
	if len(tuple) != 3 {
		return c, fmt.Errorf("invalid client tuple")
	}
	if err := json.Unmarshal(tuple[0], &c.ID); err != nil {
		return c, err
	}
	if err := json.Unmarshal(tuple[1], &c.Config); err != nil {
		return c, err
	}
	if err := json.Unmarshal(tuple[2], &c.State); err != nil {
		return c, err
	}
	return c, nil
}

func (m *Model) Apply(data []byte) error {
	var event map[string]json.RawMessage
	if err := json.Unmarshal(data, &event); err != nil {
		return err
	}
	if len(event) != 1 {
		return fmt.Errorf("invalid daemon event")
	}
	for kind, raw := range event {
		switch kind {
		case "GlobalKcp":
			return json.Unmarshal(raw, &m.GlobalKCP)
		case "ClipboardSync":
			return json.Unmarshal(raw, &m.ClipboardSync)
		case "ClipboardDeviceGrants":
			var grants map[string]bool
			if err := json.Unmarshal(raw, &grants); err != nil {
				return err
			}
			m.ClipboardGrants = grants
		case "LocalDeviceName":
			return json.Unmarshal(raw, &m.DeviceName)
		case "LocalDisplaySize":
			return json.Unmarshal(raw, &m.LocalDisplaySize)
		case "ServiceIdentity":
			var identity struct {
				Version    string
				IPCVersion uint32 `json:"ipc_version"`
			}
			if err := json.Unmarshal(raw, &identity); err != nil {
				return err
			}
			m.Version = identity.Version
			m.Online = identity.IPCVersion == FrontendIPCVersion
			if !m.Online {
				m.Error = IncompatibleDaemon
			} else if m.Error == IncompatibleDaemon {
				m.Error = ""
			}
		case "Enumerate":
			var tuples []json.RawMessage
			if err := json.Unmarshal(raw, &tuples); err != nil {
				return err
			}
			clients := map[uint64]Client{}
			for _, tuple := range tuples {
				c, err := DecodeClient(tuple)
				if err != nil {
					return err
				}
				clients[c.ID] = c
			}
			m.Clients = clients
		case "Created", "State":
			c, err := DecodeClient(raw)
			if err != nil {
				return err
			}
			m.Clients[c.ID] = c
		case "Deleted":
			var id uint64
			if err := json.Unmarshal(raw, &id); err != nil {
				return err
			}
			delete(m.Clients, id)
		case "DiscoveryEmail":
			var email *string
			if err := json.Unmarshal(raw, &email); err != nil {
				return err
			}
			m.Email = ""
			if email != nil {
				m.Email = *email
			}
			m.EmailLoaded = true
		case "NearbyDevices":
			return json.Unmarshal(raw, &m.Nearby)
		case "PairingRequests":
			return json.Unmarshal(raw, &m.Pairing)
		case "AuthorizedUpdated":
			var incoming map[string]IncomingPeer
			if err := json.Unmarshal(raw, &incoming); err != nil {
				return err
			}
			m.Incoming = incoming
			for fp := range m.Connected {
				if _, authorized := incoming[fp]; !authorized {
					delete(m.Connected, fp)
					delete(m.RemoteStates, fp)
				}
			}
		case "MdnsDiscovery":
			return json.Unmarshal(raw, &m.MDNS)
		case "CaptureStatus":
			return json.Unmarshal(raw, &m.Capture)
		case "EmulationStatus":
			return json.Unmarshal(raw, &m.Emulation)
		case "Error":
			if err := json.Unmarshal(raw, &m.Error); err != nil {
				return err
			}
			m.ErrorSequence++
		case "ReleaseThreshold":
			if err := json.Unmarshal(raw, &m.ReleaseThreshold); err != nil {
				return err
			}
			m.ThresholdLoaded = true
		case "ReleaseBind":
			if err := json.Unmarshal(raw, &m.ReleaseBind); err != nil {
				return err
			}
			m.ReleaseBindLoaded = true
		case "SuppressedAppsUpdated":
			return json.Unmarshal(raw, &m.Suppressed)
		case "RunningApps":
			return json.Unmarshal(raw, &m.Running)
		case "DeviceConnected", "DeviceEntered":
			var v struct{ Fingerprint, Addr string }
			if err := json.Unmarshal(raw, &v); err != nil {
				return err
			}
			m.Connected[v.Fingerprint] = v.Addr
		case "IncomingDisconnected":
			var addr string
			if err := json.Unmarshal(raw, &addr); err != nil {
				return err
			}
			for fp, a := range m.Connected {
				if a == addr {
					delete(m.Connected, fp)
				}
			}
		case "RemoteHostState":
			var v struct{ Fingerprint, State string }
			if err := json.Unmarshal(raw, &v); err != nil {
				return err
			}
			m.RemoteStates[v.Fingerprint] = v.State
		case "PortChanged":
			var fields []json.RawMessage
			if err := json.Unmarshal(raw, &fields); err != nil {
				return err
			}
			if len(fields) != 2 {
				return fmt.Errorf("invalid port event")
			}
			if err := json.Unmarshal(fields[0], &m.Port); err != nil {
				return err
			}
			var reason *string
			if err := json.Unmarshal(fields[1], &reason); err != nil {
				return err
			}
			if reason != nil {
				m.Error = *reason
			}
		}
	}
	return nil
}

func (m *Model) SortedClients() []Client {
	clients := make([]Client, 0, len(m.Clients))
	for _, c := range m.Clients {
		clients = append(clients, c)
	}
	sort.Slice(clients, func(i, j int) bool { return clients[i].ID < clients[j].ID })
	return clients
}
