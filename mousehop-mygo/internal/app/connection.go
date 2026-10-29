package app

import (
	"errors"
	"fmt"
	"math"
	"net"
	"net/netip"
	"slices"
	"strings"
	"time"
	"unicode"

	"github.com/alvin-7/mousehop/mygo/internal/model"
)

type connectionSettings struct {
	Hostname *string  `json:"hostname"`
	IPs      []string `json:"fix_ips"`
	Port     int      `json:"port"`
	Hook     *string  `json:"cmd"`
}

func peerConnection(peer model.Client) connectionSettings {
	return connectionSettings{peer.Config.Hostname, slices.Clone(peer.Config.IPs), peer.Config.Port, peer.Config.Hook}
}

func optionalText(value *string) string {
	if value == nil {
		return ""
	}
	return *value
}

func sameConnection(a, b connectionSettings) bool {
	left, right := slices.Clone(a.IPs), slices.Clone(b.IPs)
	slices.Sort(left)
	slices.Sort(right)
	return optionalText(a.Hostname) == optionalText(b.Hostname) && a.Port == b.Port && optionalText(a.Hook) == optionalText(b.Hook) && slices.Equal(left, right)
}

type connectionDraft struct {
	Hostname, IPs, Hook string
	Port                float64
}

func draftConnection(settings connectionSettings) connectionDraft {
	return connectionDraft{optionalText(settings.Hostname), strings.Join(settings.IPs, ", "), optionalText(settings.Hook), float64(settings.Port)}
}

func (d connectionDraft) settings() (connectionSettings, error) {
	out := connectionSettings{IPs: []string{}}
	host := strings.TrimSpace(d.Hostname)
	if len(host) > 253 || strings.ContainsFunc(host, func(ch rune) bool { return unicode.IsSpace(ch) || unicode.IsControl(ch) }) {
		return out, errors.New(tr("Enter a hostname without spaces (253 bytes maximum)"))
	}
	if host != "" {
		out.Hostname = &host
	}
	if math.IsNaN(d.Port) || d.Port < 1 || d.Port > 65535 || d.Port != math.Trunc(d.Port) {
		return out, errors.New(tr("Enter a port from 1 to 65535"))
	}
	out.Port = int(d.Port)
	for _, value := range strings.FieldsFunc(d.IPs, func(ch rune) bool { return ch == ',' || ch == ';' || unicode.IsSpace(ch) }) {
		ip, err := netip.ParseAddr(value)
		if err != nil || ip.Zone() != "" {
			return out, fmt.Errorf(tr("Invalid fixed IP address: %s"), value)
		}
		if !slices.Contains(out.IPs, ip.String()) {
			out.IPs = append(out.IPs, ip.String())
		}
	}
	if host == "" && len(out.IPs) == 0 {
		return out, errors.New(tr("Enter a hostname or at least one fixed IP address"))
	}
	if strings.TrimSpace(d.Hook) != "" {
		command := d.Hook
		if strings.ContainsRune(command, 0) {
			return out, errors.New(tr("The enter command cannot contain a NUL character"))
		}
		out.Hook = &command
	}
	return out, nil
}

type pendingConnection struct {
	Before, Target connectionSettings
	Until          time.Time
}

type pendingNetworkChoice struct {
	Before, Target string
	Until          time.Time
}

type connectionEditor struct {
	AliasEditing  bool
	AliasDraft    string
	AliasPending  *pendingAlias
	Open, Editing bool
	Draft         connectionDraft
	Pending       *pendingConnection
	Network       *pendingNetworkChoice
}

func (a *application) connectionEditor(id uint64) *connectionEditor {
	if a.connectionEditors == nil {
		a.connectionEditors = map[uint64]*connectionEditor{}
	}
	if a.connectionEditors[id] == nil {
		a.connectionEditors[id] = &connectionEditor{}
	}
	return a.connectionEditors[id]
}

func (a *application) pruneConnectionEditors() {
	for id := range a.connectionEditors {
		if _, exists := a.model.Clients[id]; !exists {
			delete(a.connectionEditors, id)
		}
	}
}

type networkChoice struct{ ID, Label string }

func networkSelection(peer model.Client) string {
	if peer.State.ActiveLock != nil {
		return "ip:" + *peer.State.ActiveLock
	}
	if peer.Config.Mode == "auto" {
		return "auto"
	}
	return "fastest"
}

func networkChoices(peer model.Client) []networkChoice {
	choices := []networkChoice{{"auto", tr("Automatic")}, {"fastest", tr("Prefer lowest latency")}}
	ips := slices.Clone(peer.State.IPs)
	ips = append(ips, peer.Config.IPs...)
	if peer.State.ActiveLock != nil {
		ips = append(ips, *peer.State.ActiveLock)
	}
	if peer.State.Address != nil {
		if ip, _, err := net.SplitHostPort(*peer.State.Address); err == nil {
			ips = append(ips, ip)
		}
	}
	slices.SortFunc(ips, func(a, b string) int {
		left, _ := netip.ParseAddr(a)
		right, _ := netip.ParseAddr(b)
		return left.Compare(right)
	})
	ips = slices.Compact(ips)
	for _, ip := range ips {
		if addr, err := netip.ParseAddr(ip); err != nil || addr.Zone() != "" {
			continue
		}
		kind := ""
		switch peer.State.Interfaces[ip] {
		case "wired":
			kind = tr(" / Wired")
		case "wifi":
			kind = tr(" / Wi-Fi")
		case "other":
			kind = tr(" / Other")
		}
		latency := tr("Not measured")
		if us, measured := peer.State.Latencies[ip]; measured {
			switch {
			case us == nil:
				latency = tr("Unreachable")
			case *us == 0:
				latency = "<0.1 ms"
			default:
				latency = fmt.Sprintf("%.1f ms", float64(*us)/1000)
			}
		}
		choices = append(choices, networkChoice{"ip:" + ip, ip + kind + " / " + latency})
	}
	return choices
}
