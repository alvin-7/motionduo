package app

import (
	"path/filepath"
	"testing"
)

func TestDefaultBackendPath(t *testing.T) {
	for _, tc := range []struct{ platform, name string }{
		{"windows", "motionduo.exe"},
		{"darwin", "motionduo"},
	} {
		got := defaultBackendPath(filepath.Join("installation", "motionduo-ui"), tc.platform)
		want := filepath.Join("installation", tc.name)
		if got != want {
			t.Errorf("%s: got %q, want %q", tc.platform, got, want)
		}
	}
}
