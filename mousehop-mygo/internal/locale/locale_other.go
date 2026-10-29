//go:build !windows && !darwin

package locale

import "os"

func SystemLocale() string { return FromEnvironment(os.Getenv) }
