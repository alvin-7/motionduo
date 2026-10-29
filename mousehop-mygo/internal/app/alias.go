package app

import (
	"errors"
	"strings"
	"time"
	"unicode"
	"unicode/utf8"

	"github.com/alvin-7/mousehop/mygo/internal/model"
	"github.com/egoist/mygo/ui"
)

type pendingAlias struct {
	Before, Target string
	Until          time.Time
}

func normalizeAlias(value string) (string, error) {
	value = strings.TrimSpace(value)
	if utf8.RuneCountInString(value) > 128 || strings.ContainsFunc(value, unicode.IsControl) {
		return "", errors.New(tr("Enter an alias of at most 128 characters without control characters"))
	}
	return value, nil
}

func (a *application) syncAlias(c *ui.Context, peer model.Client, e *connectionEditor) {
	if p := e.AliasPending; p != nil {
		switch {
		case peer.Config.Alias == p.Target:
			e.AliasPending = nil
		case peer.Config.Alias != p.Before:
			e.AliasPending = nil
			a.model.Error = tr("Alias changed elsewhere; try again")
			c.Invalidate()
		case !c.Now().Before(p.Until):
			e.AliasPending = nil
			a.model.Error = tr("Alias was not confirmed; try again")
			c.Invalidate()
		default:
			c.After(p.Until.Sub(c.Now()))
		}
	}
}

func (a *application) aliasView(c *ui.Context, peer model.Client) {
	e := a.connectionEditor(peer.ID)
	ui.Column(c).Gap(10).Disabled(!a.model.Online).Children(func() {
		if !e.AliasEditing {
			ui.Row(c).Gap(10).AlignItems(ui.Center).Children(func() {
				ui.Text(c, peer.Name()).Grow(1).MinWidth(0).MaxLines(1).Tooltip(peer.IdentityName())
				if e.AliasPending != nil {
					ui.Text(c, tr("Saving alias...")).TextColor(muted)
				}
				if ui.Button(c, tr("Edit alias")).Disabled(e.AliasPending != nil).Clicked() {
					e.AliasDraft, e.AliasEditing = peer.Config.Alias, true
				}
			})
			return
		}
		panel := ui.Column(c.Key("alias-editor")).Gap(10)
		panel.Children(func() {
			ui.TextInput(c, &e.AliasDraft).Label(tr("Local device alias")).Placeholder(tr("Alias (empty to clear)"))
			target, err := normalizeAlias(e.AliasDraft)
			if err != nil {
				ui.Text(c, err.Error()).TextColor(ui.Hex("#f09b89"))
			}
			ui.Row(c).Gap(10).Children(func() {
				if ui.PrimaryButton(c, tr("Save alias")).Disabled(err != nil || target == peer.Config.Alias || e.AliasPending != nil).Clicked() {
					if a.send("SetClientAlias", []any{peer.ID, target}) {
						e.AliasPending = &pendingAlias{peer.Config.Alias, target, c.Now().Add(3 * time.Second)}
						e.AliasEditing = false
						c.After(3 * time.Second)
					}
				}
				if ui.Button(c, tr("Cancel alias")).Clicked() {
					e.AliasEditing = false
				}
			})
		})
		if panel.Shortcut(0, ui.KeyEscape) {
			e.AliasEditing = false
			c.Invalidate()
		}
	})
}
