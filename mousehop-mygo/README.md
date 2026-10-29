# Mousehop / MyGo

The desktop frontend is native Go UI drawn by [MyGo](https://github.com/egoist/mygo),
not a webview. The previous Rust GTK frontend is removed, not retained as a fallback.

## Compact Interface

The default window is 680 x 420 DIPs (minimum 600 x 400). A slim top navigation
replaces the sidebar and duplicate page headings. Devices shows connection
approval, saved peers and nearby computers; transport labels and redundant
Arrange actions are omitted. The screen-layout canvas is 240 DIPs high, keeps
the geometry proportions, and uses drag-only magnetic corner alignment without
resolution labels or placement buttons.

Each saved or authorized device has a Configure button. It expands the device's
settings directly below that row, with only one device expanded at a time.
Outgoing options include sharing behavior and connection details; incoming
options include clipboard receiving, scrolling, sensitivity and access removal.
These controls are not in Settings > Advanced. Authorized incoming devices stay
reachable even when outgoing peers are also present. Fingerprints remain hidden.

Discovery email has one form, in Settings only. When no email is configured,
Devices offers a Set discovery email button that opens Settings, not a second
form. Nearby suppresses saved endpoints even if mDNS renames a hostname after
pairing, using a matching host or numeric address plus service port, not a
display name. This is presentation-only: it grants no access and changes no
certificate binding. Global shortcut, network and clipboard
privacy options are behind a default-closed Advanced disclosure. Input-permission
recovery remains visible when capture or emulation is unavailable. Collapsing a
device or leaving Devices closes its connection draft, not a pending save;
confirmation deadlines continue even on another page, with one scheduled wake
and no idle polling. Tray/menu-bar Open at login, Open, Restart and Quit are
unchanged. The aim is a small resident utility, not an exhaustive control panel.
Rust continues to own input capture, emulation, DTLS and KCP; the Go process talks
to it over the existing newline-delimited JSON local IPC protocol.

The visual language adapts Ballmac UI's Graphite preset, Sidebar and Card patterns
to native MyGo widgets. It does not embed React or ship Ballmac's website.
References are pinned in `THIRD_PARTY_NOTICES.md`.

## Interaction

- Configure is a keyboard-accessible disclosure: a 200 ms height reveal and
  turning arrow, with hover/pressed/focus states. Closing or changing devices
  closes drafts; controls in a closing panel are disabled. The toolkit follows
  the OS Reduce Motion preference and stops scheduling frames after settling.
- The single Settings email form disables unchanged saves, accepts Enter,
  shows Saving while waiting for the daemon, then Saved briefly after the
  canonical echo. Queue failures, rejection and a three-second echo timeout
  retain the draft and allow retry. A clean editor follows external changes;
  an unsubmitted draft is not silently overwritten.
- Nearby Connect shows Connecting and suppresses duplicate clicks until the
  canonical saved-client event. This is not a successful DTLS connection:
  the saved row still waits for actual Alive state. The three-second frontend
  command-acknowledgment deadline is separate from the backend approval retry.
  Backend failure states get short inline messages, never raw certificate text.
- Escape cancels an unsent connection draft. Dragging retains its magnetic
  preview and guides; only release commits, while Escape/blur/outside cancel.
- Feedback deadlines run even on another page. There are no spinner loops,
  idle animation timers or new protocol events for these interactions.

## Application Icons

MotionDuo uses the first double-screen flowing artwork in `assets/brand/`;
it is new artwork, not the upstream Mousehop icon. The SVGs are canonical.
The Windows ICO and macOS ICNS are generated from them. The menu bar uses a
black transparent template. Regeneration requires librsvg `rsvg-convert`;
ICO generation requires Windows PowerShell and macOS ICNS generation requires
ImageMagick and iconutil. Native appearance still requires manual acceptance.

The single-instance name, executable names, config and trust paths, Windows
install identity and macOS bundle ID/filename remain Mousehop for compatibility.
This does not prove compatibility with arbitrary older builds or installations.

To regenerate the embedded raster derivatives or inspect actual PE resources:

```sh
go -C mousehop-mygo generate
python scripts/verify-mygo-icons.py
python scripts/verify-mygo-icons.py --binary target/debug/motionduo.exe
```

## Build

From the repository root with Rust and Go 1.27.3+:

```powershell
./scripts/build-mygo.ps1
./target/debug/motionduo.exe
```

```sh
./scripts/build-mygo.sh
./target/debug/motionduo
```

Both executables must be installed together in the same directory. `motionduo`
retains the CLI and explicit `daemon` subcommand; without a subcommand it opens
`motionduo-ui`. The frontend attaches to an existing daemon or supervises its
own child. Quit stops only an owned daemon, using a process-local shutdown
capability and a bounded fallback.

## Resident App

- Windows keeps only the notification-area icon when the page is closed:
  the window is hidden, not minimized, with no remaining taskbar button.
  Clicking the tray icon opens/restores the page; right-clicking opens the menu.
- macOS keeps only its top menu-bar icon, not a Dock icon. The app uses the
  accessory activation policy and the bundle sets LSUIElement. Closing the
  page hides the window, not the app; the menu-bar menu reopens it.
- The menu contains Open MotionDuo, Open at login, Restart MotionDuo and Quit.
  Open at login is opt-in: merely launching the app never enables it. It uses
  the current user's Windows startup entry or the macOS login-item service,
  reads back OS state, and reports failures instead of leaving a false check.
  macOS login registration requires an app bundle and may require approval in
  System Settings. Startup launches leave the page parked in the background.
- Restart waits for owned-daemon shutdown before releasing the single-instance
  lock and before MyGo starts the replacement. A daemon attached independently
  is never stopped by frontend Restart/Quit; Restart restarts the frontend and
  only a daemon it owns. Normal Quit includes capability-based input/backend
  cleanup; an unresponsive owned child is force-stopped after the bounded wait.
- If tray creation fails, the window is shown and closing it exits, so an
  unavailable resident entry does not leave an inaccessible process.

Resident menu/window tests use adapters, including system-setting failures and
external changes to the startup checkbox. Supervisor subprocess tests verify
graceful owner-capability shutdown, forced fallback and attach-without-ownership.
These do not constitute native login/restart/tray acceptance. A resumed Windows
native probe successfully observed the compact pages, hid the window on close
while its process remained alive, and reopened the same isolated `--no-daemon`
instance. The available Computer Use inventory did not expose the notification
area, so tray-menu clicks and restart remain unverified. Fixtures were cleaned
up without changing startup registration or installed apps.
The frontend also cross-compiles for darwin/arm64 with CGO disabled; this is
compile-only evidence, not a Mac backend/package build or native runtime test.

## Discovery And Approval

Enter the same email on each computer on the same LAN. An email-derived SHA-256
namespace is advertised using mDNS. There is no account, email verification or
cloud discovery. Email is stored in local configuration; its hash on the LAN
is not a secret and can be guessed. Do not use it as an authentication factor.

Discovery shows nearby computers, but never authorizes control. A selects Connect;
B sees the requesting computer and chooses Allow or Decline. The fingerprint is
kept out of normal UI. Allow persists the actual certificate rejected by the DTLS
verifier, not a certificate authorized solely from an mDNS advertisement. Names
and advertised addresses are unverified discovery metadata. A retries the
handshake for up to one minute while waiting for approval. The approval grants
A control of B; the reverse direction needs its own approval. Clipboard is off
until explicitly enabled on both sending and receiving sides.

New requests restore the window even when it was parked in the background. Repeated
updates of the same request do not continually steal focus. A device selected
from discovery also pins B's advertised certificate for the outgoing handshake;
the pin survives restarts and is not silently replaced when the certificate
changes. Remove and add the device again after an intentional identity reset.
Legacy/manual clients without a pin retain their existing behavior.

## Screen Layout

Thumbnails normalize the shared edge instead of sharing a pixels-to-DIPs scale:
left/right neighbors have equal heights, top/bottom neighbors have equal widths.
Each screen retains its aspect ratio. Different resolutions with the same aspect
ratio therefore use identical thumbnail sizes. Missing or zero geometry uses the
known peer's aspect ratio, or a neutral placeholder when both are unknown.
Normalization is visual only; actual display geometry and crossing transforms
are unchanged, and opening the page sends no layout-setting requests.

Drag the remote screen around the stationary local screen. There are no
direction or corner placement buttons. The nearest left/right/top/bottom edge
and corner appear as a ghost outline with an alignment guide; within 18 DIPs,
the screen magnetically attaches to that corner while the pointer moves. Drop
commits the displayed edge and anchor together. With several configured peers,
the Computer dropdown only selects which remote screen to arrange.
The remote thumbnail rescales when snapping between horizontal and vertical
neighbors without moving the local screen. Since equal edges make both corner
outlines overlap, a slight cross-axis offset chooses the top/bottom or left/right
guide. A centered drag retains the existing anchor rather than flipping it.

Escape, switching pages, losing focus, going offline or releasing outside the
canvas cancels the gesture. A click is not a drag. Pointer motion sends no IPC
requests: one drop sends one `UpdateScreenPlacement` request, and the daemon
updates both fields before broadcasting or recreating capture. A rejected or
unconfirmed save reverts to the daemon's layout instead of showing it as saved.
Confirmation and timeout handling follow the peer that originated the drop,
even after switching pages or selecting another computer. Deleting that peer
cancels its gesture and pending save before a reused handle can inherit them.
The frontend IPC revision is now **4**; older native-preview daemons are rejected
rather than receiving a request they cannot decode. The DTLS peer wire protocol
is unchanged.

Corner alignment preserves cross-axis distance in backend display-coordinate units;
it does not pretend that pixels on two monitors have the same physical size.
Only the shared part of a mismatched edge permits crossing. Landing fractions
use the same source topology snapshot as the crossing event, and the sender's
return model applies the inverse transform. Peer geometry is announced during
the existing Hello exchange before the first aligned crossing. This does not
change DTLS/KCP event serialization or the peer on-wire protocol version.
Legacy proportional mappings remain intact until the user drops a new corner
placement; they are not silently rewritten on opening the page.

The diagram uses the daemon's local capture geometry and the peer's announced
dimensions, including unequal-size corner anchors. Dimensions affect the
diagram proportions but are not printed as secondary labels. Missing dimensions
use a neutral placeholder, not an invented resolution.

## Return Control

Settings > Advanced includes a shortcut editor with independent left/right Ctrl, Shift,
Alt/Option and Win/Command/Super checkboxes, plus a searchable trigger-key
picker. Modifier-only chords and standalone trigger keys are supported.
The picker uses the daemon's shared scancode enum names, with tests checking
each offered key against `input-event/src/scancode.rs`. Existing uncommon or
multi-trigger bindings survive editing until a replacement trigger is chosen.
There is no naive recorder that turns an aggregate modifier flag into the
wrong left/right key.

Apply shortcut sends one `SetReleaseBind` request. Cancel or leaving Settings
discards the shortcut draft. Use default sends an empty array, which restores
the daemon's built-in left Ctrl + Shift + Meta + Alt chord; it does not disable
release. The current shortcut stays daemon-confirmed while a save is pending.

Wall-press fallback is a separate pixel threshold for cases where a peer cannot
return control, not the normal screen-crossing threshold. Zero disables only
this fallback. The editor preserves the full existing unsigned 32-bit range
instead of clamping values to the old slider's 500-pixel limit. Editing or
scrolling the page sends no changes until Apply threshold is clicked.

Both settings wait for their canonical daemon events before becoming editable.
Queued changes await an echo; a send failure leaves the draft available, while
a three-second confirmation timeout reverts to daemon state and reports an
error. One scheduled wake handles each timeout, not continuous idle polling.
Disconnect clears pending edits and requires fresh settings on reconnect.

## Tests And Isolation

```sh
go -C mousehop-mygo test ./...
cargo test --workspace
python scripts/test-mygo-lan.py
```

UI tests use MyGo's in-memory native renderer and do not touch a running app.
Set `MOUSEHOP_TEST_SNAPSHOTS` to an output directory for fixture PNGs. Fixtures
are test data, not evidence of a real dual-machine session.

For an isolated daemon test, set `MOUSEHOP_IPC_ADDR=127.0.0.1:<unused-port>` on
Windows or `MOUSEHOP_SOCKET_PATH=<absolute-path>` on Unix, pass a separate config
and certificate path, use a separate UDP port, and select dummy backends. Never
reuse the installed app's mDNS name: set a unique `MOUSEHOP_DISCOVERY_NAME`
(a DNS label, maximum 63 ASCII letters/digits/hyphens). Never
point a test at installed production configuration. The frontend accepts
`--no-daemon` and `--backend <path>` for supervised development and QA.

The LAN smoke verifies the spawned daemon PID before any mutable IPC request,
so a raced port cannot accidentally change an installed daemon. It records
evidence under `target/mygo-qa/` and shuts down only its own children. This is
same-host mDNS/DTLS testing, not two physical computers or real input injection.
It also verifies sided release shortcuts, default reset and wall-press threshold
enable/disable against canonical events and the isolated saved configuration.

UI mutations require a compatible `ServiceIdentity.ipc_version=4` handshake.

The page header and local layout screen use `LocalDeviceName` from the daemon,
the same name it advertises to Nearby peers. Long names stay on one line with
the full name available on hover. Application, tray and menu-bar branding stays
Mousehop. Removing incoming access revokes live sessions as well as future TLS
handshakes; queued input from retired sessions is discarded.
An older daemon already occupying the normal IPC endpoint is not upgraded,
reconfigured or stopped automatically; the UI asks you to quit the other app.

## Migration Status

CI and release recipes now build/package both executables; Windows packaging no
longer stages GTK DLLs. `scripts/package-macos.sh` stages a MyGo-main app bundle,
rejects non-system dylibs and derives its minimum OS from both Mach-O binaries.
Linux's native MyGo platform integration still needs GTK3, not the removed Rust
GTK4 frontend. Nix/Flatpak/AUR recipes have been converted but not built here.
Nix needs a nixpkgs toolchain providing Go 1.27.3+ and its vendor hash must be
confirmed by a real Nix build. No workflows or release uploads were triggered.

Devices > Configure > Connection details includes host/IP/port and an optional local enter-command
editing. One `UpdateConnection` saves these fields atomically while preserving
identity, sharing permissions and layout. Drafts wait for canonical state; failed
sends retain the draft. The network picker keeps stable address identities while
RTT labels change; an offline pinned address remains selectable. Endpoint changes
reset the old transport, and stale DNS responses for another hostname are ignored.

macOS input-access onboarding is now implemented, but not runtime-validated on
a Mac. A small card appears only when access is missing, cannot be checked, or
a permission change needs a restart. It checks silently at startup and when
the app becomes active, with an explicit Check again fallback; there is no
frontend permission polling timer. Open settings requests the missing permission
at most once per run, and only after that click. Accessibility is checked and
requested before any CoreGraphics listen/post access, avoiding duplicate prompts.
System API request return values are never treated as approval. After grants,
Restart MotionDuo uses the existing owned-daemon shutdown/relaunch path; an
independently attached daemon is still not stopped. No device is authorized by
an OS permission action, and no permission action runs automatically at launch.

Run the packaged `.app` for native TCC acceptance: terminal-launched development
executables can inherit the terminal's responsibility/grants. If System Settings
does not list the app, add `MotionDuo.app` manually in Accessibility. The frontend
checks its current responsibility identity, not the daemon's effective backend
state; it never marks capture/emulation enabled solely from a preflight. A clean
daemon exit (including permission revocation) is not auto-restarted and now
reports the stopped input service instead of silently showing Reconnecting.

Migration remains in progress: live macOS permission, window/tray/platform
tests are not yet complete. About/version presentation can
remain a low-frequency follow-up rather than expanding the main interface.
Windows/macOS physical crossing/return, Chinese IME and
idle CPU/memory still require acceptance. Do not treat this branch as a release.

## Code organization

- `main.go`: executable entry point; retains version/commit linker variables.
- `internal/app`: application lifecycle, page views and UI interaction tests.
- `internal/model`: daemon event decoding and frontend state.
- `internal/ipc`: daemon connection and platform endpoints.
- `internal/locale`: translations and platform locale detection.

Manual connections are added from the **+** button at the right of Nearby on the Devices page. The dialog accepts an address and port, with Add and Cancel actions; Escape or clicking outside also dismisses it.
