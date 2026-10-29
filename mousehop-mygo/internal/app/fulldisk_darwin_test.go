//go:build darwin

package app

import (
	"errors"
	"syscall"
	"testing"
)

func TestFullDiskAccessFromErrors(t *testing.T) {
	for _, test := range []struct {
		name   string
		errors []error
		want   bool
	}{
		{name: "system denied and user missing", errors: []error{syscall.EPERM, syscall.ENOENT}, want: false},
		{name: "any success", errors: []error{syscall.EPERM, nil, syscall.ENOENT}, want: true},
		{name: "all missing", errors: []error{syscall.ENOENT, syscall.ENOENT, syscall.ENOENT}, want: true},
		{name: "Safari denied only", errors: []error{syscall.ENOENT, errors.Join(errors.New("open failed"), syscall.EACCES), syscall.ENOENT}, want: false},
	} {
		t.Run(test.name, func(t *testing.T) {
			if got := fullDiskAccessFromErrors(test.errors); got != test.want {
				t.Fatalf("fullDiskAccessFromErrors(%v) = %v, want %v", test.errors, got, test.want)
			}
		})
	}
}
