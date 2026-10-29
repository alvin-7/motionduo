package app

import (
	"context"
	"encoding/json"
	"fmt"
	"net"
	"os"
	"path/filepath"
	"runtime"
	"testing"
	"time"
)

func isolatedEndpoint(t *testing.T) (string, string) {
	t.Helper()
	if runtime.GOOS != "windows" {
		// Darwin limits Unix socket paths to 103 bytes; CI TMPDIR can exceed it.
		dir, err := os.MkdirTemp("/tmp", "motionduo-ipc-")
		if err != nil {
			t.Fatal(err)
		}
		t.Cleanup(func() { _ = os.RemoveAll(dir) })
		path := filepath.Join(dir, "ipc.sock")
		t.Setenv("MOUSEHOP_SOCKET_PATH", path)
		return "unix", path
	}
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	address := listener.Addr().String()
	_ = listener.Close()
	t.Setenv("MOUSEHOP_IPC_ADDR", address)
	return "tcp", address
}

// A test subprocess, not an input backend: it accepts only its owner's
// shutdown capability. No installed daemon, config or autostart is touched.
func publishHelperMarker(path string) error {
	temporary := path + ".tmp"
	if err := os.WriteFile(temporary, []byte(fmt.Sprint(os.Getpid())), 0600); err != nil {
		return err
	}
	return os.Rename(temporary, path)
}

func TestDaemonHelperProcess(t *testing.T) {
	if os.Getenv("MOUSEHOP_TEST_HELPER") != "1" {
		return
	}
	marker := os.Getenv("MOUSEHOP_TEST_MARKER")
	if os.Getenv("MOUSEHOP_TEST_CLEAN_EXIT") == "1" {
		if publishHelperMarker(marker) != nil {
			os.Exit(5)
		}
		os.Exit(0)
	}
	if os.Getenv("MOUSEHOP_TEST_STUBBORN") == "1" {
		if publishHelperMarker(marker) != nil {
			os.Exit(5)
		}
		for {
			time.Sleep(time.Hour)
		}
	}
	network, address := "unix", os.Getenv("MOUSEHOP_SOCKET_PATH")
	if runtime.GOOS == "windows" {
		network, address = "tcp", os.Getenv("MOUSEHOP_IPC_ADDR")
	}
	listener, err := net.Listen(network, address)
	if err != nil {
		os.Exit(2)
	}
	if publishHelperMarker(marker) != nil {
		os.Exit(5)
	}
	conn, err := listener.Accept()
	if err != nil {
		os.Exit(3)
	}
	_ = conn.SetReadDeadline(time.Now().Add(10 * time.Second))
	var request map[string]string
	err = json.NewDecoder(conn).Decode(&request)
	_ = conn.Close()
	_ = listener.Close()
	if err != nil || request["Shutdown"] == "" || request["Shutdown"] != os.Getenv("MOUSEHOP_OWNER_TOKEN") {
		os.Exit(4)
	}
	_ = os.WriteFile(marker+".stopped", []byte("graceful"), 0600)
	os.Exit(0)
}

func TestSupervisorReportsCleanExitWithoutRestartingOrChangingPermissions(t *testing.T) {
	isolatedEndpoint(t)
	marker := filepath.Join(t.TempDir(), "exited")
	t.Setenv("MOUSEHOP_TEST_HELPER", "1")
	t.Setenv("MOUSEHOP_TEST_MARKER", marker)
	t.Setenv("MOUSEHOP_TEST_CLEAN_EXIT", "1")
	executable, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	var reported string
	s := newDaemonSupervisor(executable, []string{"-test.run=^TestDaemonHelperProcess$", "--"}, func(message string) { reported = message })
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	finished := make(chan struct{})
	go func() { s.run(ctx); close(finished) }()
	t.Cleanup(s.stop)
	select {
	case <-finished:
	case <-time.After(10 * time.Second):
		t.Fatal("clean exit was automatically restarted")
	}
	if reported != "Input service stopped. Restart MotionDuo from the tray or menu bar." {
		t.Fatal("daemon stop was silently presented as reconnecting", reported)
	}
	data, err := os.ReadFile(marker)
	if err != nil || string(data) != fmt.Sprint(s.child.Process.Pid) {
		t.Fatal("exit marker does not belong to the isolated child", err)
	}
}

func TestSupervisorStopsOwnedChildBeforeReturning(t *testing.T) {
	for _, stubborn := range []bool{false, true} {
		t.Run(fmt.Sprint(stubborn), func(t *testing.T) {
			isolatedEndpoint(t)
			marker := filepath.Join(t.TempDir(), "ready")
			t.Setenv("MOUSEHOP_TEST_HELPER", "1")
			t.Setenv("MOUSEHOP_TEST_MARKER", marker)
			if stubborn {
				t.Setenv("MOUSEHOP_TEST_STUBBORN", "1")
			}
			executable, err := os.Executable()
			if err != nil {
				t.Fatal(err)
			}
			s := newDaemonSupervisor(executable, []string{"-test.run=^TestDaemonHelperProcess$", "--"}, func(string) {})
			ctx, cancel := context.WithCancel(context.Background())
			finished := make(chan struct{})
			go func() { s.run(ctx); close(finished) }()
			t.Cleanup(func() { cancel(); s.stop(); <-finished })
			deadline := time.Now().Add(10 * time.Second)
			for {
				if data, err := os.ReadFile(marker); err == nil {
					s.mu.Lock()
					pid := s.child.Process.Pid
					s.mu.Unlock()
					if string(data) != fmt.Sprint(pid) {
						t.Fatal("helper marker does not belong to supervised child")
					}
					break
				}
				if time.Now().After(deadline) {
					t.Fatal("helper did not start")
				}
				time.Sleep(10 * time.Millisecond)
			}
			cancel()
			s.stop()
			select {
			case <-s.done:
			default:
				t.Fatal("shutdown returned before owned child exited")
			}
			if !stubborn {
				if _, err := os.Stat(marker + ".stopped"); err != nil {
					t.Fatal("shutdown did not use the child's actual owner capability")
				}
			}
			s.stop() // OnQuit and deferred cleanup must be safe to repeat.
		})
	}
}

func TestSupervisorDoesNotStopAnAttachedDaemon(t *testing.T) {
	network, address := isolatedEndpoint(t)
	listener, err := net.Listen(network, address)
	if err != nil {
		t.Fatal(err)
	}
	defer listener.Close()
	accepted := make(chan net.Conn, 1)
	go func() {
		conn, err := listener.Accept()
		if err == nil {
			accepted <- conn
		}
	}()
	s := newDaemonSupervisor("must-not-execute", nil, func(string) {})
	s.run(context.Background())
	s.stop()
	if s.child != nil {
		t.Fatal("attaching must not claim ownership")
	}
	select {
	case conn := <-accepted:
		defer conn.Close()
		_ = conn.SetReadDeadline(time.Now().Add(time.Second))
		var data [1]byte
		if n, _ := conn.Read(data[:]); n != 0 {
			t.Fatal("attach probe sent a mutable request")
		}
	case <-time.After(time.Second):
		t.Fatal("no attach probe")
	}
	conn, err := net.DialTimeout(network, address, time.Second)
	if err != nil {
		t.Fatal("attached endpoint was stopped")
	}
	_ = conn.Close()
}
