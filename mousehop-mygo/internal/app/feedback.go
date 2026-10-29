package app

import (
	"net/netip"
	"strings"
	"time"

	"github.com/alvin-7/mousehop/mygo/internal/model"
	"github.com/egoist/mygo/ui"
)

type pendingEmail struct {
	Before, Target string
	Until          time.Time
	ErrorSequence  uint64
}

type pendingDiscovery struct {
	Device        model.DiscoveredDevice
	Until         time.Time
	ErrorSequence uint64
}

func normalizedEmail(value string) string { return strings.ToLower(strings.TrimSpace(value)) }

func (a *application) syncEmail(c *ui.Context) {
	if !a.model.Online {
		a.emailPending, a.emailSavedUntil = nil, time.Time{}
		return
	}
	if !a.model.EmailLoaded {
		return
	}
	if !a.emailInitialized {
		a.emailDraft, a.emailObserved, a.emailInitialized = a.model.Email, a.model.Email, true
	} else if a.model.Email != a.emailObserved {
		if a.emailDraft == a.emailObserved || a.emailPending != nil && a.emailPending.Target == a.model.Email {
			a.emailDraft = a.model.Email
		}
		a.emailObserved = a.model.Email
	}
	if pending := a.emailPending; pending != nil {
		switch {
		case a.model.Email == pending.Target:
			a.emailPending, a.emailDraft = nil, a.model.Email
			a.emailSavedUntil = c.Now().Add(2 * time.Second)
		case a.model.ErrorSequence != pending.ErrorSequence, a.model.Email != pending.Before:
			a.emailPending = nil
		case !c.Now().Before(pending.Until):
			a.emailPending = nil
			a.model.Error = tr("Email was not confirmed; try again")
		default:
			c.After(pending.Until.Sub(c.Now()))
		}
	}
	if c.Now().Before(a.emailSavedUntil) {
		c.After(a.emailSavedUntil.Sub(c.Now()))
	} else {
		a.emailSavedUntil = time.Time{}
	}
}

func (a *application) syncDiscovery(c *ui.Context) {
	if !a.model.Online {
		a.discoveryPending = nil
		return
	}
	for id, pending := range a.discoveryPending {
		confirmed := false
		for _, peer := range a.model.Clients {
			confirmed = confirmed || matchesDiscovered(peer, pending.Device)
		}
		nearby := false
		for _, device := range a.model.Nearby {
			nearby = nearby || device.ID == id
		}
		switch {
		case confirmed, !nearby, a.model.ErrorSequence != pending.ErrorSequence:
			delete(a.discoveryPending, id)
		case !c.Now().Before(pending.Until):
			delete(a.discoveryPending, id)
			a.model.Error = tr("Connection request was not confirmed; try again")
		default:
			c.After(pending.Until.Sub(c.Now()))
		}
	}
}

func matchesDiscovered(peer model.Client, device model.DiscoveredDevice) bool {
	if peer.Config.Port != device.Port {
		return false
	}
	if peer.Config.Hostname != nil && device.Hostname != "" && strings.EqualFold(strings.TrimSuffix(*peer.Config.Hostname, "."), strings.TrimSuffix(device.Hostname, ".")) {
		return true
	}
	// mDNS may rename a host after pairing. Endpoint matching only hides a
	// duplicate row; it never authorizes a device or changes its pinned identity.
	ips := append([]string{}, peer.Config.IPs...)
	ips = append(ips, peer.State.IPs...)
	if peer.State.Address != nil {
		if active, err := netip.ParseAddrPort(*peer.State.Address); err == nil {
			ips = append(ips, active.Addr().String())
		}
	}
	for _, address := range device.Addresses {
		discovered, err := netip.ParseAddr(address)
		if err != nil {
			continue
		}
		for _, ip := range ips {
			if saved, err := netip.ParseAddr(ip); err == nil && saved.Unmap() == discovered.Unmap() {
				return true
			}
		}
	}
	return false
}

func deviceConnectionStatus(peer model.Client) (string, bool) {
	if peer.Config.ApprovalPaused {
		return tr("Approval required"), true
	}
	if peer.State.Alive {
		return tr("Connected"), false
	}
	if strings.HasPrefix(peer.State.Transport, "Failed:") || peer.State.LastError != nil {
		reason := strings.ToLower(peer.State.Transport)
		if peer.State.LastError != nil {
			reason += " " + strings.ToLower(*peer.State.LastError)
		}
		switch {
		case strings.Contains(reason, "identity"), strings.Contains(reason, "certificate"):
			return tr("Device identity needs confirmation"), true
		case strings.Contains(reason, "release"):
			return tr("Input release needs attention"), true
		case strings.Contains(reason, "timeout"), strings.Contains(reason, "timed out"), strings.Contains(reason, "unreachable"):
			return tr("Connection timed out"), true
		default:
			return tr("Connection failed"), true
		}
	}
	if peer.State.Active {
		return tr("Waiting for connection"), false
	}
	return tr("Not connected"), false
}
