mod approval;
mod capture;
pub mod capture_test;
pub mod client;
#[allow(dead_code)]
mod clipboard_focus;
#[cfg(any(windows, target_os = "macos"))]
mod clipboard_focus_native;
#[allow(dead_code)]
mod clipboard_session;
// Isolated transfer prototype; deliberately not wired into the service yet.
#[allow(dead_code)]
mod clipboard_transfer;
pub mod config;
mod connect;
mod crypto;
#[cfg(all(unix, not(target_os = "macos")))]
pub mod desktop_install;
mod discovery;
mod dns;
mod emulation;
pub mod emulation_test;
pub mod firewall;
mod latency;
mod listen;
#[cfg(target_os = "macos")]
mod macos_power;
#[cfg(target_os = "macos")]
pub mod macos_tcc_probe;
#[cfg(target_os = "macos")]
pub mod macos_tcc_watch;
mod nearby;
mod network;
pub mod panic_log;
pub mod service;
mod transport;
#[cfg(windows)]
pub mod windows_console;
