package ipc

import (
	"bufio"
	"context"
	"encoding/json"
	"net"
	"testing"
	"time"
)

func TestIPCRequestWireFormatAndGenerationIsolation(t *testing.T) {
	p := New()
	if err := p.Send("Create", nil); err == nil {
		t.Fatal("offline request accepted")
	}
	uiConn, daemonConn := net.Pipe()
	defer daemonConn.Close()
	p.conn, p.generation = uiConn, 2
	p.requests <- request{data: []byte("old request\n"), generation: 1}
	if err := p.Send("RespondPairing", []any{"opaque-id", true}); err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	p.OnEvent = func([]byte) error { return nil }
	done := make(chan error, 1)
	go func() { done <- p.serve(ctx, uiConn, 2) }()
	_ = daemonConn.SetReadDeadline(time.Now().Add(time.Second))
	line, err := bufio.NewReader(daemonConn).ReadBytes('\n')
	if err != nil {
		t.Fatal(err)
	}
	var request map[string]json.RawMessage
	if err := json.Unmarshal(line, &request); err != nil {
		t.Fatal(err)
	}
	if string(request["RespondPairing"]) != `["opaque-id",true]` {
		t.Fatal(string(line))
	}
	cancel()
	select {
	case <-done:
	case <-time.After(time.Second):
		t.Fatal("reader did not shut down")
	}
}

func TestIPCQueueIsBounded(t *testing.T) {
	p := New()
	a, b := net.Pipe()
	defer a.Close()
	defer b.Close()
	p.conn = a
	for i := 0; i < 64; i++ {
		if err := p.Send("Sync", nil); err != nil {
			t.Fatal(err)
		}
	}
	if err := p.Send("Sync", nil); err == nil {
		t.Fatal("full queue accepted request")
	}
}
