package app

import (
	"encoding/json"
	"errors"
	"strings"
	"testing"
	"time"

	"github.com/alvin-7/mousehop/mygo/internal/locale"

	"github.com/egoist/mygo/ui"
)

func TestAliasValidationAndIdentity(t *testing.T) {
	for _, value := range []string{"", "  ", "  工作电脑  ", strings.Repeat("名", 128)} {
		got, err := normalizeAlias(value)
		if err != nil || got != strings.TrimSpace(value) {
			t.Fatal(got, err)
		}
	}
	for _, value := range []string{strings.Repeat("名", 129), "a\nb", "a\x00b"} {
		if _, err := normalizeAlias(value); err == nil {
			t.Fatal("accepted invalid alias")
		}
	}
	peer := fixture().model.Clients[7]
	identity := peer.Name()
	peer.Config.Alias = "工作电脑"
	if peer.Name() != "工作电脑" || peer.IdentityName() != identity {
		t.Fatal("alias replaced identity")
	}
	peer.Config.Alias = ""
	if peer.Name() != identity {
		t.Fatal("empty alias did not restore identity")
	}
}

func aliasTester(a *application) *ui.Tester {
	return ui.NewTester(func(c *ui.Context) {
		theme := graphiteTheme()
		c.SetTheme(&theme)
		a.syncConnection(c, a.model.Clients[7], a.connectionEditor(7))
		ui.Column(c).Padding(14).Children(func() { a.aliasView(c, a.model.Clients[7]) })
	}, 540, 400)
}

func TestAliasSaveEchoClearAndQueueFailure(t *testing.T) {
	a := fixture()
	var calls []string
	a.request = func(kind string, value any) error {
		data, _ := json.Marshal(map[string]any{kind: value})
		calls = append(calls, string(data))
		return nil
	}
	tt := aliasTester(a)
	clickRelease(t, tt, "Edit alias")
	clickRelease(t, tt, "Local device alias")
	tt.Type("  Office  ")
	if len(calls) != 0 {
		t.Fatal("typing sent mutation")
	}
	clickRelease(t, tt, "Save alias")
	e := a.connectionEditor(7)
	if len(calls) != 1 || calls[0] != `{"SetClientAlias":[7,"Office"]}` || e.AliasPending == nil || a.model.Clients[7].Config.Alias != "" {
		t.Fatal(calls, e.AliasPending)
	}
	peer := a.model.Clients[7]
	peer.Config.Alias = "Office"
	a.model.Clients[7] = peer
	tt.Frame()
	if e.AliasPending != nil {
		t.Fatal("echo not acknowledged")
	}
	clickRelease(t, tt, "Edit alias")
	e.AliasDraft = "  "
	tt.Frame()
	clickRelease(t, tt, "Save alias")
	if calls[1] != `{"SetClientAlias":[7,""]}` || a.model.Clients[7].Config.Alias != "Office" {
		t.Fatal(calls)
	}
	peer.Config.Alias = ""
	a.model.Clients[7] = peer
	tt.Frame()
	clickRelease(t, tt, "Edit alias")
	e.AliasDraft = "Retry"
	tt.Frame()
	a.request = func(string, any) error { return errors.New("queue full") }
	clickRelease(t, tt, "Save alias")
	if !e.AliasEditing || e.AliasPending != nil || e.AliasDraft != "Retry" {
		t.Fatal("queue failure lost draft")
	}
}

func TestAliasHiddenTimeoutPruneAndOffline(t *testing.T) {
	a := fixture()
	e := a.connectionEditor(7)
	e.AliasPending = &pendingAlias{"", "Office", time.Now().Add(time.Hour)}
	a.page = "Settings"
	tt := ui.NewTester(a.view, minWindowWidth, minWindowHeight)
	e.AliasPending.Until = tt.Now().Add(time.Second)
	tt.Frame()
	tt.Advance(2 * time.Second)
	if e.AliasPending != nil || !tt.HasText("Alias was not confirmed") || tt.FramePending() {
		t.Fatal(tt.Texts())
	}
	e.AliasEditing = true
	a.closeDeviceDrafts()
	if e.AliasEditing {
		t.Fatal("hidden draft survived")
	}
	if _, err := a.receiveEvent([]byte(`{"Deleted":7}`)); err != nil {
		t.Fatal(err)
	}
	if len(a.connectionEditors) != 0 {
		t.Fatal("deleted handle retained editor")
	}
	a = fixture()
	a.model.Online = false
	sent := false
	a.request = func(string, any) error { sent = true; return nil }
	tt = aliasTester(a)
	tt.Click("Edit alias")
	tt.Frame()
	if sent || a.connectionEditor(7).AliasEditing {
		t.Fatal("offline alias editable")
	}
}

func TestAliasLocales(t *testing.T) {
	before := locale.Language
	defer func() { locale.Language = before }()
	for _, key := range []string{"Edit alias", "Local device alias", "Alias (empty to clear)", "Save alias", "Cancel alias", "Saving alias...", "Alias was not confirmed; try again", "Alias changed elsewhere; try again", "Enter an alias of at most 128 characters without control characters"} {
		locale.Language = "zh-Hans"
		if tr(key) == key {
			t.Fatal(key)
		}
		locale.Language = "en"
		if tr(key) != key {
			t.Fatal(key)
		}
	}
}
