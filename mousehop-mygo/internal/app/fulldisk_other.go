//go:build !darwin

package app

func fullDiskAccess() bool          { return true }
func fullDiskAccessSupported() bool { return false }
