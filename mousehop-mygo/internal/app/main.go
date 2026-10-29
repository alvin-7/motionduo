package app

import (
	"context"
	"fmt"
	"log"
	"os"
	"path/filepath"
	"runtime"

	"github.com/egoist/mygo"
)

var version = "development"

// Override at build time for isolated visual previews without taking the resident lock.
var applicationName = "Mousehop"

func defaultBackendPath(executable, platform string) string {
	name := "motionduo"
	if platform == "windows" {
		name += ".exe"
	}
	return filepath.Join(filepath.Dir(executable), name)
}

func Run(v, revision, name string) {
	version, commit, applicationName = v, revision, name
	if len(os.Args) == 2 && os.Args[1] == "--version" {
		fmt.Println("MotionDuo " + version)
		return
	}
	mygo.App.SetName(applicationName)
	mygo.App.SetActivationPolicy(residentActivationPolicy(runtime.GOOS))
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	app := newApplication()
	executable, err := os.Executable()
	if err != nil {
		log.Fatal(err)
	}
	backend := defaultBackendPath(executable, runtime.GOOS)
	args := []string{}
	spawn := true
	for i := 1; i < len(os.Args); i++ {
		switch os.Args[i] {
		case "--no-daemon":
			spawn = false
		case "--backend":
			i++
			if i >= len(os.Args) {
				log.Fatal("--backend needs a path")
			}
			backend = os.Args[i]
		default:
			args = append(args, os.Args[i])
		}
	}
	var daemon *daemonSupervisor
	if spawn {
		daemon = newDaemonSupervisor(backend, args, app.reportError)
		defer daemon.stop()
	}
	// MyGo relaunches after OnQuit, before Run returns. Clean up before
	// registering the lock-release listener, not just in a deferred call.
	mygo.App.OnQuit(func() {
		cancel()
		if daemon != nil {
			daemon.stop()
		}
	})
	if !mygo.App.RequestSingleInstanceLock() {
		return
	}
	// Keep the old lock key while exposing the new name to native UI backends.
	if applicationName == "Mousehop" {
		mygo.App.SetName("MotionDuo")
	}
	mygo.App.WhenReady(func() {
		app.ready(ctx)
		if daemon != nil {
			go daemon.run(ctx)
		}
	})
	if err := mygo.App.Run(); err != nil {
		log.Print(err)
	}
}
