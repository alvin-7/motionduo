package app

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"fmt"
	"os"
	"os/exec"
	"sync"
	"time"

	"github.com/alvin-7/mousehop/mygo/internal/ipc"
)

type daemonSupervisor struct {
	mu       sync.Mutex
	backend  string
	args     []string
	report   func(string)
	child    *exec.Cmd
	done     chan struct{}
	stopping bool
	token    string
}

// report surfaces a lifecycle failure to the user; it may be called from the
// supervisor goroutine.
func newDaemonSupervisor(backend string, args []string, report func(string)) *daemonSupervisor {
	var token [32]byte
	if _, err := rand.Read(token[:]); err != nil {
		panic(err)
	}
	return &daemonSupervisor{backend: backend, args: args, report: report, token: hex.EncodeToString(token[:])}
}

func (s *daemonSupervisor) run(ctx context.Context) {
	// Attach to an existing daemon without taking ownership of its lifetime.
	probeCtx, cancel := context.WithTimeout(ctx, time.Second)
	conn, err := ipc.DialDaemon(probeCtx)
	cancel()
	if err == nil {
		_ = conn.Close()
		return
	}
	for ctx.Err() == nil {
		s.mu.Lock()
		if s.stopping {
			s.mu.Unlock()
			return
		}
		args := append(append([]string{}, s.args...), "daemon")
		cmd := exec.Command(s.backend, args...)
		cmd.Env = append(os.Environ(), "MOUSEHOP_OWNER_TOKEN="+s.token)
		cmd.Stdout, cmd.Stderr = os.Stdout, os.Stderr
		hideConsole(cmd)
		err := cmd.Start()
		if err != nil {
			s.mu.Unlock()
			s.report(fmt.Sprintf("Cannot start daemon: %v", err))
			return
		}
		s.child, s.done = cmd, make(chan struct{})
		done := s.done
		s.mu.Unlock()
		err = cmd.Wait()
		close(done)
		if err == nil {
			s.mu.Lock()
			stopping := s.stopping
			s.mu.Unlock()
			if !stopping && ctx.Err() == nil {
				s.report("Input service stopped. Restart MotionDuo from the tray or menu bar.")
			}
			return
		} // Permission revocation / intentional shutdown.
		select {
		case <-ctx.Done():
			return
		case <-time.After(2 * time.Second):
		}
	}
}

func (s *daemonSupervisor) stop() {
	s.mu.Lock()
	s.stopping = true
	child, done := s.child, s.done
	s.mu.Unlock()
	if child == nil {
		return
	}
	select {
	case <-done:
		return
	default:
	}
	// Only our capability can shut down our child. A daemon that won a
	// startup race or was already running does not accept this request.
	ctx, cancel := context.WithTimeout(context.Background(), time.Second)
	defer cancel()
	if conn, err := ipc.DialDaemon(ctx); err == nil {
		_ = conn.SetWriteDeadline(time.Now().Add(time.Second))
		fmt.Fprintf(conn, "{\"Shutdown\":\"%s\"}\n", s.token)
		_ = conn.Close()
	}
	select {
	case <-done:
	case <-time.After(3 * time.Second):
		_ = child.Process.Kill()
		<-done
	}
}
