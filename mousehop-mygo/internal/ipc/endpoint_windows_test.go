//go:build windows

package ipc

import (
	"context"
	"testing"
)

func TestIPCOverrideRefusesNonLoopbackOrNonNumericEndpoints(t *testing.T) {
	for _, address := range []string{"192.168.1.20:5252", "0.0.0.0:5252", "localhost:5252", "127.0.0.1:0", "127.0.0.1:http", "127.0.0.1:65536", "[::1]:-1"} {
		t.Run(address, func(t *testing.T) {
			t.Setenv("MOUSEHOP_IPC_ADDR", address)
			if conn, err := DialDaemon(context.Background()); err == nil {
				conn.Close()
				t.Fatal("invalid IPC override accepted")
			}
		})
	}
}
