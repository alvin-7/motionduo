# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.17.19] - 2026-10-26

### Fixed

- Discard deferred network-policy requests when a client is removed or rebuilt
  on configuration reload, and reject requests for missing clients before
  enqueueing. This prevents a reused client handle from inheriting old requests.
- Includes all network refresh and GUI responsiveness changes from 0.17.18.

### Release status

- The 0.17.18 tag was pushed, but its draft release was withdrawn after this
  additional pre-publication review. The immutable tag remains for traceability;
  0.17.19 is the published version of this repair.

### Validation

- Final 0.17.19 workspace tests: Windows 315 passed, macOS Apple Silicon 355
  passed; both passed Clippy. The same explicit environment-sensitive test skip
  and live-acceptance limitations documented for 0.17.18 remain in effect.

## [0.17.18] - 2026-10-26

### Fixed

- Move blocking interface and gateway discovery off the input executor. Share
  snapshots between network locks and mDNS, coalesce refreshes, and avoid
  re-registering unchanged advertisements without extending KCP timeouts.
- Preserve listeners on enumeration failure and discard snapshots superseded by
  interface events. Defer network-policy changes until a fresh query completes.
- Move GTK daemon request writes to a bounded background FIFO, report queue
  saturation, and disconnect partial writes so the frontend can reconnect.
- Defer macOS activation-policy updates out of window visibility callbacks and
  add close/visibility diagnostics. Closing still hides the window, not quits.

### Validation

- Windows: 314 workspace tests passed; macOS Apple Silicon: 354 passed. Both
  platforms passed Clippy and the real interface-query scheduling probe.
- One network-environment-sensitive latency test was explicitly filtered on each
  platform; other ignored tests are documented in the repair report.
- Long-running wired dual-machine input and the intermittent local-trackpad
  close-button failure still require live acceptance. The GUI changes harden
  identified risks; the intermittent close failure's root cause is unconfirmed.

## [0.17.17] - 2026-10-20

### Fixed

- Retain macOS remote modifier-release ownership until a bounded asynchronous
  retry and HID/session readback confirm the released side cleared. Keep failed
  or timed-out releases in cleanup residue instead of reporting a clean state.
- Explicitly reconcile movement, button/drag and scroll modifier flags with the
  current remote plan and distinguishable local input, rather than inheriting
  stale remote Ctrl flags. Preserve local opposite-side modifiers.

### Validation

- Add twelve modifier regressions covering delayed release, stale pointer flags,
  sibling preservation, re-press, idle release, timeout, API failure,
  cancellation and chord cleanup. Real dual-machine acceptance remains a
  separate check; automated tests alone do not prove the intermittent issue is
  eliminated.

## [0.17.16] - 2026-10-15

### Changed

- Make the existing 600ms KCP input-stall baseline explicit in the example configuration and document updating older, explicitly configured 300ms installations on both endpoints. Preserve the 3000ms peer deadline and all KCP/UDP transport parameters.

## [0.17.15] - 2026-10-15

### Fixed

- Stop key repeat before releasing held input when control leaves an endpoint. Retain failed key, mouse-button and native macOS modifier releases for retry instead of discarding their handle.
- Pause new input and entry warps while a previous handle still has unconfirmed cleanup; retry in the serial emulation worker without restarting the backend. Share macOS exit and termination cleanup.

## [0.17.14] - 2026-10-15

### Fixed

- Synchronize macOS lock state before enabling a newly registered capture edge, so an unlock missed while disconnected cannot permanently block returning to the controlling machine.
- Retain lock protection when reconnecting to a locked Mac, and queue the initial state before the edge's first capture event.
- Recheck disabled macOS event taps after unlock, notify capture of Windows session lock changes, and retry emulation after local unlock or transient Windows input failures.

## [0.17.13](https://github.com/alvin-7/mousehop/compare/v0.17.12...v0.17.13) - 2026-09-28

### Fixed

- Release Windows modifiers that were already held before crossing to a remote screen, preventing Ctrl from remaining active after return.
- Recover stalled reliable input on the existing Windows-to-macOS connection when the macOS desktop can provide a live display and cursor baseline. Recovery has a fixed two-second deadline.
- Stop macOS key repeat before recovery cleanup and report failed mouse-button or modifier injection instead of treating it as successful cleanup.

- Correct the cargo-bundle key for the macOS `LSUIElement` / TCC plist: it belongs under `[package.metadata.bundle.osx]` as `info_plist_exts`, so the bundled `.app` was previously built without it.

### Changed

- Fix the modifier release order on screen handover: reset the aggregate modifier mask before sending individual key-ups. macOS reads combined flags on each transition, so releasing a chord one key at a time could reclassify a just-released remote flag as locally held.

## [0.17.12] - 2026-09-23

Everything from 0.17.2 through 0.17.12 was developed on this fork without
per-release changelog entries. The list below covers that whole span; see the
`README.md` and `DOC.md` sections named after each version for the details.

### Added

- Experimental reliable input over DTLS: a KCP session carries keyboard and
  clipboard events while pointer motion stays on plain UDP, with per-client
  opt-in, connection recovery and separate connection-liveness tracking.
- Per-client scroll inertia and dual-axis scroll direction, configurable from
  the GTK preferences window and the config file.
- macOS keyboard injection via an IOKit `NXEventData` bridge, so system
  shortcuts and cross-platform modifier aliasing work on a macOS receiver.
- macOS trackpad pinch-to-zoom capture and steadier trackpad scrolling, using a
  vendored `core-graphics` that exposes the `CGEventTypeGesture` variant.
- A per-user Windows x64 installer (`scripts/package-windows.ps1` plus
  `packaging/windows.iss`) and an embedded application icon for `mousehop.exe`.
- Persistent panic and input-release logging for diagnosing crashes after the
  fact.

### Fixed

- Keep the Windows shared UDP listener receiving after `WSAECONNRESET` (10054).
  Windows surfaces a peer's ICMP Port Unreachable through `recv_from`, and
  tearing down the shared receive task left the port bound but every later DTLS
  handshake unanswered. Patched in the vendored `webrtc-util`.
- Bundle transitive dylib dependencies and prepare the icon correctly in the
  macOS packaging scripts.

## [0.11.3](https://github.com/jondkinney/mousehop/compare/v0.11.2...v0.11.3) - 2026-05-20

### Added

- *(desktop)* install desktop entry on first launch
- *(tray)* recolor tray glyph at runtime via currentColor

## [0.11.2] - 2026-05-19

### Fixed

- Flatpak icon validation: GdkPixbuf's SVG sniffer reads only the first
  ~256 bytes looking for the `<svg` tag, and the multi-line XML
  docstring above the root element pushed it past that window. Moved
  the docstring inside `<svg>` so Flatpak's icon validator accepts the
  app icon and the Flatpak bundle exports cleanly.

## [0.11.1] - 2026-05-19

### Fixed

- Typo "occured" → "occurred" in the `IpcError::Io` thiserror message
  (user-visible in logs and CLI output), a doc comment on
  `FrontendEvent::Error`, and the `capture_event_occured` local
  variable in `input-capture`'s libei capture loop.
