//go:build windows

package ipc

import (
	"context"
	"fmt"
	"net"
	"os"
	"strconv"
)

func DialDaemon(ctx context.Context) (net.Conn, error) {
	address := os.Getenv("MOUSEHOP_IPC_ADDR")
	if address == "" {
		address = "127.0.0.1:5252"
	}
	host, port, err := net.SplitHostPort(address)
	ip := net.ParseIP(host)
	number, portErr := strconv.Atoi(port)
	if err != nil || ip == nil || !ip.IsLoopback() || portErr != nil || number < 1 || number > 65535 {
		return nil, fmt.Errorf("MOUSEHOP_IPC_ADDR must be a loopback IP and nonzero numeric port")
	}
	return (&net.Dialer{}).DialContext(ctx, "tcp", address)
}
