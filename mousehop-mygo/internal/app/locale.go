package app

import "github.com/alvin-7/mousehop/mygo/internal/locale"

func tr(english string) string               { return locale.Tr(english) }
func trf(english string, args ...any) string { return locale.Trf(english, args...) }
