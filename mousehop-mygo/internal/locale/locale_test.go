package locale

import "testing"

func TestLocaleSelection(t *testing.T) {
	for locale, want := range map[string]string{"zh_CN.UTF-8": "zh-Hans", "zh-SG": "zh-Hans", "zh-Hans-CN": "zh-Hans", "zh": "zh-Hans", "zh-TW": "en", "zh-Hant": "en", "en-US": "en", "fr_FR": "en", "C": "en", "": "en"} {
		if got := LanguageForLocale(locale); got != want {
			t.Errorf("%q: %s, want %s", locale, got, want)
		}
	}
	env := map[string]string{"LC_ALL": "en_US", "LC_MESSAGES": "zh_CN", "LANG": "fr_FR"}
	get := func(k string) string { return env[k] }
	if FromEnvironment(get) != "en_US" {
		t.Fatal("LC_ALL not honored")
	}
	delete(env, "LC_ALL")
	if FromEnvironment(get) != "zh_CN" {
		t.Fatal("LC_MESSAGES not honored")
	}
	delete(env, "LC_MESSAGES")
	if FromEnvironment(get) != "fr_FR" {
		t.Fatal("LANG not honored")
	}
}
