//go:build !windows

package ipc

import (
	"context"
	"fmt"
	"net"
	"os"
	"path/filepath"
	"runtime"
)

func DialDaemon(ctx context.Context) (net.Conn, error) {
	path := os.Getenv("MOUSEHOP_SOCKET_PATH")
	if path == "" {
		if runtime.GOOS == "darwin" {
			home, err := os.UserHomeDir()
			if err != nil {
				return nil, err
			}
			path = filepath.Join(home, "Library", "Caches", "mousehop-socket.sock")
		} else {
			dir := os.Getenv("XDG_RUNTIME_DIR")
			if dir == "" {
				return nil, fmt.Errorf("XDG_RUNTIME_DIR is not set")
			}
			path = filepath.Join(dir, "mousehop-socket.sock")
		}
	}
	return (&net.Dialer{}).DialContext(ctx, "unix", path)
}
