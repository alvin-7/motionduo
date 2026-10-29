package app

import (
	"fmt"
	"math"
	"slices"
	"strings"
	"time"
)

type Key struct{ Code, Label string }

var Modifiers = []Key{
	{"KeyLeftCtrl", "Left Ctrl"}, {"KeyRightCtrl", "Right Ctrl"},
	{"KeyLeftShift", "Left Shift"}, {"KeyRightShift", "Right Shift"},
	{"KeyLeftAlt", "Left Alt"}, {"KeyRightalt", "Right Alt"},
	{"KeyLeftMeta", "Left Super"}, {"KeyRightmeta", "Right Super"},
}

var DefaultBind = []string{"KeyLeftCtrl", "KeyLeftShift", "KeyLeftMeta", "KeyLeftAlt"}

const NoTrigger = "Modifiers only"

var Triggers = func() []Key {
	keys := []Key{}
	for ch := 'A'; ch <= 'Z'; ch++ {
		keys = append(keys, Key{"Key" + string(ch), string(ch)})
	}
	for i := 0; i <= 9; i++ {
		keys = append(keys, Key{fmt.Sprintf("Key%d", i), fmt.Sprint(i)})
	}
	for i := 1; i <= 24; i++ {
		keys = append(keys, Key{fmt.Sprintf("KeyF%d", i), fmt.Sprintf("F%d", i)})
	}
	return append(keys,
		Key{"KeyEsc", "Escape"}, Key{"KeyEnter", "Enter"}, Key{"KeyTab", "Tab"},
		Key{"KeySpace", "Space"}, Key{"KeyBackspace", "Backspace"}, Key{"KeyDelete", "Delete"},
		Key{"KeyInsert", "Insert"}, Key{"KeyHome", "Home"}, Key{"KeyEnd", "End"},
		Key{"KeyPageup", "Page Up"}, Key{"KeyPagedown", "Page Down"},
		Key{"KeyUp", "Up arrow"}, Key{"KeyDown", "Down arrow"},
		Key{"KeyLeft", "Left arrow"}, Key{"KeyRight", "Right arrow"},
		Key{"KeyMinus", "Minus"}, Key{"KeyEqual", "Equal"},
		Key{"KeyLeftbrace", "Left bracket"}, Key{"KeyRightbrace", "Right bracket"},
		Key{"KeyBackslash", "Backslash"}, Key{"KeySemicolon", "Semicolon"},
		Key{"KeyApostrophe", "Apostrophe"}, Key{"KeyGrave", "Grave"},
		Key{"KeyComma", "Comma"}, Key{"KeyDot", "Period"}, Key{"KeySlash", "Slash"},
		Key{"KeyCapsLock", "Caps Lock"}, Key{"KeyNumlock", "Num Lock"},
		Key{"KeyScrollLock", "Scroll Lock"}, Key{"KeyPause", "Pause"},
	)
}()

func KeyLabel(code, platform string) string {
	for _, key := range Modifiers {
		if key.Code == code {
			label := key.Label
			if platform == "darwin" {
				label = strings.ReplaceAll(label, "Super", "Command")
				label = strings.ReplaceAll(label, "Alt", "Option")
			} else if platform == "windows" {
				label = strings.ReplaceAll(label, "Super", "Win")
			}
			return label
		}
	}
	for _, key := range Triggers {
		if key.Code == code {
			return key.Label
		}
	}
	return code
}

func ChordLabel(keys []string, platform string) string {
	labels := make([]string, len(keys))
	for i, key := range keys {
		labels[i] = KeyLabel(key, platform)
	}
	return strings.Join(labels, " + ")
}

func SameBind(a, b []string) bool {
	a, b = slices.Clone(a), slices.Clone(b)
	slices.Sort(a)
	slices.Sort(b)
	return slices.Equal(a, b)
}

type ShortcutEditor struct {
	Open          bool
	Modifiers     [8]bool
	Trigger       string
	ExistingLabel string
	ExistingKeys  []string
}

func NewShortcutEditor(keys []string) ShortcutEditor {
	e := ShortcutEditor{Open: true, Trigger: NoTrigger}
	for _, code := range keys {
		index := slices.IndexFunc(Modifiers, func(k Key) bool { return k.Code == code })
		if index >= 0 {
			e.Modifiers[index] = true
		} else {
			e.ExistingKeys = append(e.ExistingKeys, code)
		}
	}
	if len(e.ExistingKeys) != 0 {
		e.ExistingLabel = ChordLabel(e.ExistingKeys, "")
		e.Trigger = e.ExistingLabel
	}
	return e
}

func (e *ShortcutEditor) Options() []string {
	options := []string{NoTrigger}
	for _, key := range Triggers {
		options = append(options, key.Label)
	}
	if e.ExistingLabel != "" && !slices.Contains(options, e.ExistingLabel) {
		options = append(options, e.ExistingLabel)
	}
	return options
}

func (e *ShortcutEditor) Keys() []string {
	keys := []string{}
	for i, selected := range e.Modifiers {
		if selected {
			keys = append(keys, Modifiers[i].Code)
		}
	}
	if e.Trigger == e.ExistingLabel && e.ExistingLabel != "" {
		// Keep unusual or multi-trigger bindings until the user chooses a replacement.
		return append(keys, e.ExistingKeys...)
	}
	for _, key := range Triggers {
		if key.Label == e.Trigger {
			return append(keys, key.Code)
		}
	}
	return keys
}

type PendingShortcut struct {
	Before, Target []string
	Until          time.Time
}

type PendingThreshold struct {
	Target uint32
	Until  time.Time
}

type ThresholdEditor struct {
	Initialized bool
	Confirmed   uint32
	Value       float64
	Pending     *PendingThreshold
}

func (e *ThresholdEditor) Sync(actual uint32, now time.Time) bool {
	if !e.Initialized {
		*e = ThresholdEditor{Initialized: true, Confirmed: actual, Value: float64(actual)}
	}
	if p := e.Pending; p != nil {
		switch {
		case actual == p.Target, actual != e.Confirmed:
			e.Pending, e.Confirmed, e.Value = nil, actual, float64(actual)
		case !now.Before(p.Until):
			e.Pending, e.Value = nil, float64(actual)
			return false
		}
	} else if actual != e.Confirmed {
		if e.Value == float64(e.Confirmed) {
			e.Value = float64(actual)
		}
		e.Confirmed = actual
	}
	return true
}

func ThresholdValue(value float64) (uint32, bool) {
	if math.IsNaN(value) || math.IsInf(value, 0) || value < 0 || value > math.MaxUint32 || value != math.Trunc(value) {
		return 0, false
	}
	return uint32(value), true
}
