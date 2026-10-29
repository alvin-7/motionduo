package app

import (
	"encoding/json"
	"testing"

	"github.com/egoist/mygo/ui"
)

func TestDevicesRejectsOccupiedSideAndShowsName(t *testing.T) {
	for _, language := range []string{"en", "zh-Hans"} {
		t.Run(language, func(t *testing.T) {
			useLanguage(t, language)
			a := fixture()
			a.model.Pairing = nil
			occupied := a.model.Clients[7]
			occupied.Config.Alias = "Office"
			a.model.Clients[7] = occupied
			peer := occupied
			peer.ID, peer.Config.Alias, peer.State.Active = 8, "Second", false
			a.model.Clients[8] = peer
			writes := 0
			a.request = func(string, any) error { writes++; return nil }
			tt := ui.NewTester(a.view, 680, 900)
			clickRelease(t, tt, trf("Share with %s", "Second"))
			tt.Frame()
			if writes != 0 || !tt.HasText(trf("Disable %s first", "Office")) || a.model.Clients[8].State.Active {
				t.Fatal("occupied switch did not show conflict", writes, tt.Texts())
			}
		})
	}
}

func TestDevicesAllowsFreeSideEnableAndConflictingDisable(t *testing.T) {
	for _, tc := range []struct {
		name     string
		active   bool
		position string
		want     string
	}{
		{"enable free", false, "left", `{"Activate":[8,true]}`},
		{"disable occupied", true, "right", `{"Activate":[8,false]}`},
	} {
		t.Run(tc.name, func(t *testing.T) {
			a := fixture()
			a.model.Pairing = nil
			peer := a.model.Clients[7]
			peer.ID, peer.Config.Alias, peer.Config.Position, peer.State.Active = 8, "Second", tc.position, tc.active
			a.model.Clients[8] = peer
			var calls []string
			a.request = func(kind string, value any) error {
				data, err := json.Marshal(map[string]any{kind: value})
				if err != nil {
					t.Fatal(err)
				}
				calls = append(calls, string(data))
				return nil
			}
			tt := ui.NewTester(a.view, 680, 900)
			clickRelease(t, tt, trf("Share with %s", "Second"))
			if len(calls) != 1 || calls[0] != tc.want {
				t.Fatal("incorrect switch request", calls, tc.want)
			}
		})
	}
}
