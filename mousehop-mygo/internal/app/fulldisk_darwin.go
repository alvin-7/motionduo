//go:build darwin

package app

import (
	"errors"
	"os"
	"path/filepath"
	"syscall"
)

func fullDiskAccess() bool {
	paths := []string{"/Library/Application Support/com.apple.TCC/TCC.db"}
	home, homeErr := os.UserHomeDir()
	if homeErr == nil {
		paths = append(paths,
			filepath.Join(home, "Library", "Safari"),
			filepath.Join(home, "Library", "Application Support", "com.apple.TCC", "TCC.db"),
		)
	}
	probeErrors := make([]error, 0, len(paths)+1)
	for _, path := range paths {
		file, err := os.Open(path)
		if err == nil {
			_ = file.Close()
		}
		probeErrors = append(probeErrors, err)
	}
	if homeErr != nil {
		probeErrors = append(probeErrors, homeErr)
	}
	return fullDiskAccessFromErrors(probeErrors)
}

func fullDiskAccessFromErrors(probeErrors []error) bool {
	denied := false
	for _, err := range probeErrors {
		if err == nil {
			return true
		}
		if errors.Is(err, syscall.EPERM) || errors.Is(err, syscall.EACCES) {
			denied = true
		}
	}
	return !denied
}

func fullDiskAccessSupported() bool { return true }
