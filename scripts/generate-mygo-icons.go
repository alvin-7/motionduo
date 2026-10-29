//go:build ignore

// Run from the mygo module with go generate; requires rsvg-convert (librsvg).
package main

import (
	"fmt"
	"os"
	"os/exec"
)

func main() {
	for _, job := range []struct{ source, output, size string }{
		{"../assets/brand/motionduo.svg", "../assets/brand/motionduo-512.png", "512"},
		{"../assets/brand/motionduo.svg", "internal/app/assets/app.png", "256"},
		{"../assets/brand/motionduo-tray.svg", "internal/app/assets/menubar.png", "64"},
	} {
		cmd := exec.Command("rsvg-convert", "-w", job.size, "-h", job.size, job.source, "-o", job.output)
		cmd.Stderr = os.Stderr
		if err := cmd.Run(); err != nil {
			fmt.Fprintln(os.Stderr, err)
			os.Exit(1)
		}
	}
}
