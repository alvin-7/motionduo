package locale

import (
	"context"
	"os/exec"
	"strings"
	"time"
)

func SystemLocale() string {
	ctx, cancel := context.WithTimeout(context.Background(), time.Second)
	defer cancel()
	out, err := exec.CommandContext(ctx, "/usr/bin/defaults", "read", "-g", "AppleLanguages").Output()
	if err != nil {
		return ""
	}
	// defaults prints the ordered preference array. Only the first language
	// controls the interface; a secondary Chinese preference must not override it.
	fields := strings.FieldsFunc(string(out), func(r rune) bool {
		return r == '(' || r == ')' || r == ',' || r == '"' || r == '\n' || r == ' ' || r == '\t'
	})
	if len(fields) == 0 {
		return ""
	}
	return fields[0]
}
