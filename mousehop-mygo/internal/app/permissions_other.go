//go:build !darwin

package app

func newPlatformPermissions() *Controller { return nil }
