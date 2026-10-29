package ipc

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net"
	"sync"
	"time"
)

type request struct {
	data       []byte
	generation uint64
}

// Each connection owns its reader and writer. Bounded queues never block a
// UI frame, and commands from a previous daemon generation cannot be replayed.
type Client struct {
	mu         sync.Mutex
	conn       net.Conn
	generation uint64
	requests   chan request
	dial       func(context.Context) (net.Conn, error)
	OnEvent    func([]byte) error
	OnStatus   func(bool, error)
}

func New() *Client { return &Client{requests: make(chan request, 64), dial: DialDaemon} }

func (p *Client) Send(kind string, value any) error {
	var payload any = kind
	if value != nil {
		payload = map[string]any{kind: value}
	}
	data, err := json.Marshal(payload)
	if err != nil {
		return err
	}
	p.mu.Lock()
	defer p.mu.Unlock()
	if p.conn == nil {
		return fmt.Errorf("Daemon is not connected")
	}
	select {
	case p.requests <- request{append(data, '\n'), p.generation}:
		return nil
	default:
		return fmt.Errorf("Too many pending changes; try again")
	}
}

func (p *Client) Run(ctx context.Context) {
	for ctx.Err() == nil {
		conn, err := p.dial(ctx)
		if err != nil {
			p.OnStatus(false, err)
			select {
			case <-ctx.Done():
				return
			case <-time.After(time.Second):
				continue
			}
		}
		p.mu.Lock()
		p.conn = conn
		p.generation++
		generation := p.generation
		p.mu.Unlock()
		p.OnStatus(true, nil)
		_ = p.Send("Sync", nil)
		err = p.serve(ctx, conn, generation)
		_ = conn.Close()
		p.mu.Lock()
		p.conn = nil
		p.mu.Unlock()
		p.OnStatus(false, err)
	}
}

func (p *Client) serve(ctx context.Context, conn net.Conn, generation uint64) error {
	readResult := make(chan error, 1)
	readerDone := make(chan struct{})
	defer func() { _ = conn.Close(); <-readerDone }()
	go func() {
		defer close(readerDone)
		scanner := bufio.NewScanner(conn)
		scanner.Buffer(make([]byte, 4096), 4*1024*1024)
		for scanner.Scan() {
			if err := p.OnEvent(scanner.Bytes()); err != nil {
				readResult <- err
				return
			}
		}
		err := scanner.Err()
		if err == nil {
			err = io.EOF
		}
		readResult <- err
	}()
	for {
		select {
		case <-ctx.Done():
			return ctx.Err()
		case err := <-readResult:
			return err
		case req := <-p.requests:
			if req.generation != generation {
				continue
			}
			if err := conn.SetWriteDeadline(time.Now().Add(time.Second)); err != nil {
				return err
			}
			if _, err := io.Copy(conn, bytes.NewReader(req.data)); err != nil {
				return err
			}
		}
	}
}
