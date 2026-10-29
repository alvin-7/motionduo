# Mousehop Handoff

## 2026-10-29: MotionDuo 1.1.0 prerelease

- Published `v1.1.0` from main `fb9c2fe270f1c7d243e73d5f7fd72f0e6bfc3428`, with Windows x64 and Mac ARM assets only. CI run `40840277382` had successful Windows, Mac ARM, and Ubuntu jobs at publication; Intel was still running and is not claimed as verified. The user explicitly excluded Intel from this release gate without cancelling its job.
- Nix Binary Cache is a separate Nix build workflow, and release-plz is a separate crate automation workflow; neither is the user-approved Windows/Mac ARM prerelease gate. Release-plz failed for missing `RELEASE_PLZ_TOKEN`; do not claim all workflows passed or add publication credentials without authorization. The AUR workflow listens to published releases without a prerelease guard; its downstream result is not verified.
- Clean installations backed up configuration and certificates. Windows retained the old `Mousehop` Run entry pointing at the removed old UI; it was not automatically migrated. Mac capture reports `Disabled`; verify accessibility permissions and actual interaction manually, without programmatic TCC changes. IPC connectivity is not a substitute for dual-device input or clipboard acceptance.

## 2026-10-29: macOS CI Unix socket paths

- macOS CI Go supervisor tests failed with `bind: invalid argument` because Go's test temporary directory made the Unix socket path exceed Darwin's limit. Keep isolated test sockets in a short temporary directory; do not rely on the runner's `TMPDIR` length. The same long-path failure was reproduced on native Mac.
- The separate `release-plz` workflow on the fork failed before publishing because `RELEASE_PLZ_TOKEN` was empty (`GITHUB_TOKEN is empty`). Do not add credentials or rerun its crate publication without explicit authorization; this is distinct from the matrix CI result.

## 2026-10-29: Windows CI Go formatting

- Windows CI checkout converted tracked LF Go files to CRLF, so `gofmt -l .` reported `mousehop-mygo/main.go` despite no semantic or index diff. Keep `*.go text eol=lf` in `.gitattributes` so Windows checkouts preserve the format required by CI.

## 2026-10-29: 1.1.0 release task in progress, not published

- The Go refactor is incomplete: IPC and platform endpoints/tests moved to `mousehop-mygo/internal/ipc`, locale selection/dictionary and platform detection moved to `internal/locale`, and application state moved out of `main.go`. The remaining frontend views/model/layout/permissions are still mostly flat. Preserve the pre-existing clipboard and settings changes; do not treat this partial refactor as release-ready.
- Workspace package/dependency versions and Windows manifest now say 1.1.0; `Cargo.lock` was updated during `cargo check --workspace --offline`. `cargo fmt --all -- --check` passed, but the check, workspace tests and Clippy were interrupted while still running; none is a pass. Complete Rust validation and the release pipeline must still be confirmed before commit or publication.
- Go 1.27.3 Windows `go vet ./...`, `go test ./...`, and Darwin ARM64 cross-build passed. Native Mac Go tests passed from isolated `~/.cache/mousehop/release-1.1.0-refactor-20261029-110626/` with `TMPDIR=/tmp`; the earlier frontend-only snapshot failed because it omitted referenced repository assets and used a long temporary Unix socket path. Those failures are not product failures.
- No commit, push, tag, release, package or installation was made by this attempt. The frontend refactor must be completed and revalidated before continuing. Keep `target/` and this local handoff out of release commits unless explicitly intended; preserve all existing user changes.

## 2026-10-28: focus clipboard sync investigation (incomplete)

- Production clipboard sync remains disabled; no focus-sync implementation or native acceptance is claimed. Preserve the existing uncommitted clipboard work while completing the task in `target/codex-tasks/clipboard-focus-sync.md`.
- `clipboard_focus.rs` currently contains test-only state primitives, not a transport, worker backend, or production manager. The state tests do not exercise lane cancellation, asynchronous races, session identity, or OS clipboard access. Do not enable Service based on these tests.
- Windows workspace tests after these primitives: 441 passed, 3 ignored; strict workspace Clippy and fmt check passed. On an isolated Mac source snapshot at `/tmp/mousehop-focus-sync-20261028-223338`, parallel mousehop tests failed in `fake_backend_bidirectional_through_real_bridge` (245 passed, 1 failed, 1 ignored); serial rerun passed mousehop 246/1 ignored and input-capture 71, and strict targeted Clippy passed. The parallel failure remains unexplained; neither run tests the new focus design end to end or accesses the real clipboard.
- `ImageReader::read_current` is an unintegrated bounded worker API that does not consume a generation or apply echo suppression; it checks privacy before and after reading. It is not a generation cache, does not return the blocked generation on a privacy rejection, and has not been tested against a native clipboard. The focus manager must still implement the approved cache and peer authorization before using it.
- The task's later design decisions explicitly select a new event on every outbound Begin after the session check, and the existing incoming `CaptureBegin` return path, accepting extra triggers; these decisions supersede the earlier caution about requiring proof of completed handover.
- `ClientEntered` is emitted only when `active_client` changes and before the new Enter is sent or acknowledged. A fresh Begin can reuse the same handle, so that event alone is not a reliable notification for every crossing. `CaptureBegin` for an incoming EnterOnly handle is emitted before the no-active-client check and sends a Leave through Service; it is not itself proof that handover completed. Audit actual return/release paths and exact session identity before binding clipboard work to these notifications.
- The existing session rejects a new Offer while one is active, and its Offer contains format and size but no content hash; it cannot directly satisfy supersession or the requested future hash-based announcement. The current reader consumes generations and uses the old echo/gate machinery. Replacing only the Service factory would enable the wrong model and is unsafe.

## 2026-10-28: concurrent clipboard updates

- The current bridge can rebind an unstarted local sender to a newer peer generation. Thus a delayed local sample can overwrite a newer remote update; enqueue ordering alone does not guarantee convergence. Simply discarding the sender broke bidirectional concurrent-update tests and was reverted. Native Service remains disabled pending an authenticated deterministic conflict policy and regression tests.
- The newer shared macOS ClipboardGate covers writer publication, capture, and guarded sample admission; the older review below predates it. It does not itself establish distributed update ordering, native pasteboard acceptance, or cross-process persistence. Counter reset remains fail-closed. A Windows test rerun encountered LNK1104 from a locked executable; do not count that attempt as passed.

## 2026-10-28: macOS generation-fence review

- The newer macOS generation high-water fence replaces the earlier permanent marker blacklist, but is NOT safe to enable alone: remote payload may be published before receipt registration, registration can fail, and a capture can complete before the fence is installed. Suppress before native publication and serialize final capture publication against the same lifecycle gate, including failure paths; validate on actual OS before enabling.
- Counter reset or wrap may indefinitely suppress subsequent generations under a monotonic high-water policy; no reset protocol has been verified. Same-ownership local mutations are intentionally suppressed. Service remains disabled.

## 2026-10-28: macOS capture provenance validation

- Latest input-capture was compiled and tested on ARM64 via `ssh mac` in an isolated source snapshot: 69 tests passed and strict all-target/all-feature Clippy passed. Tests did not exercise the system pasteboard or the installed app.
- `NSPasteboard.changeCount` tracks ownership changes, not necessarily each mutation by the same owner; a stable count alone cannot establish atomic marker/content provenance. The current capture path conservatively rejects tagged samples and leaves untagged external capture intact. This is not an operational macOS synchronization backend; do not claim the outstanding Mac provenance problem is solved.
- The section below supersedes the older orchestration section regarding owner namespaces and immediate owner invalidation; earlier snapshots are historical.

## 2026-10-28: service clipboard orchestration remains native-disabled

- Added an injectable, eight-peer bounded orchestration step and fake bidirectional capture/bridge/write regression. Production Service instantiates only the disabled backend; no native synchronization, receipt registration or target-device acceptance is claimed.
- Owner identity includes inbound/outbound direction plus exact authenticated session, so independent session counters cannot alias. Transport lane close publishes an atomic invalidation before queue cleanup; queued write permits check that exact owner flag at commit, without waiting for the next service tick. Revocation does not wait on a worker mutex. Writes already inside commit are irreversible and linearized before revocation; fake tests do not establish OS behavior.
- Focused clipboard suite passes 46 tests including namespace/owner-token regressions. Strict all-feature library Clippy now passes; the fake-only mutex and commit helper are test-gated rather than retained as unused production code. Windows adapter review: asynchronous write failures are only logged and orchestration discards backend admission errors, so there is no write-completion/retry contract. Receipt-registration rejection is an error and the Windows writer clears its partial publication while still holding the native lock; it is not a successful write with an unregistered content receipt. Clearing may itself change the clipboard, so do not assume rollback restores prior content or prevents all subsequent capture. Production stays disabled pending native lifecycle/echo acceptance, error-completion integration, privacy policy integration, stable bounded owner discovery and physical acceptance. No input-capture, input-emulation or macOS file was edited by this orchestration task.

## 2026-10-28: macOS receipt adapter opt-in integration

- The isolated macOS receipt writer is now compiled only for macOS + `clipboard-image` and exposed through `ClipboardEmulation::set_with_receipt_checked(MacClipboardFormat, Vec<u8>, guard, registered) -> Result<Option<MacWriteReceipt>, ClipboardError>`. A process-wide single-admission worker holds an owned semaphore permit through blocking work even after async cancellation. Registration runs synchronously under the lifecycle guard with canonical bytes; false returns an error, not an undo of the admitted OS write. Existing arboard APIs/capability query and Windows APIs are unchanged. No service enables it.
- Native ARM64 integration validation uses `/tmp/mousehop-clipboard-macos-AiUF6P/integration`: clipboard tests 16 passed; strict all-target/all-feature crate Clippy and feature-off compilation passed. Windows focused clipboard tests 11 passed, one destructive native test ignored; strict crate Clippy passed. Existing Mac IOHID deprecation/archive-tool probe warnings remain. Tests never access the real pasteboard. Native runtime, Objective-C exception behavior, hard timeout, capture registration wiring and dual-device acceptance remain unverified.
- SSH alias uses zsh where `status` is read-only; use another shell result variable for scripted checks. Preserve concurrent Windows `NativeWriteReceipt` export in clipboard.rs.

## 2026-10-28: clipboard orchestration and Mac access

- The service orchestration is disabled in production: fake-backend tests do not establish native synchronization. Owner replacement is observed on the service tick, and inbound/outbound session counters can collide for the same fingerprint; resolve both before enabling a native backend. A native write already admitted cannot be retracted by revocation.
- Windows MSVC clipboard-image tests reported 80 passed and 2 ignored; real system clipboard acceptance has not been run. The Windows prototype publishes PNG as the registered PNG format and requires guarded receipt registration. Capture-side provenance matching is not integrated.
- `ssh mac` reaches the ARM64 Mac; direct IP authentication failed because it did not use the alias configuration. Noninteractive SSH PATH does not find Rust: the installed native toolchain is `~/.rustup/toolchains/1.95.0-aarch64-apple-darwin/bin`. Mac native validation and dual-device acceptance remain outstanding. Do not package or install this feature as complete yet.

## 2026-10-28: native write receipt capability

- `ClipboardEmulation::verified_native_write_receipt_support()` explicitly reports unsupported on Windows, macOS and other platforms. It is a capability query, not an implementation or production integration; the existing arboard writes cannot prove which native generation belongs to the write.
- A Windows native clipboard transaction or macOS content-and-marker approach needs platform-specific implementation and actual target validation; neither has been shown impossible. Do not infer a receipt by polling a sequence number after writing. Focused emulation tests (8) and targeted strict Clippy passed; no native clipboard or dual-device acceptance was performed.


## 2026-10-28: authenticated clipboard lane ownership

- Transport now wraps explicit Legacy connections in the shared Peer path while preserving Legacy input semantics. Clipboard remains disabled unless local Hello and immutable peer capability both agree.
- Clipboard lane acquisition is one-shot. Outbound and inbound owners retain the actual DTLS peer-certificate fingerprint, never the expected pin. Acquisition and revocation require the exact connection session; replacement, disconnect and close synchronously invalidate old queues before disposal.
- Focused validation: clipboard transport 40 tests, real DTLS loopback fingerprint assertion 1, connect tests 14, listener tests 7. Returned bytes, in-flight packets and native writes cannot be retracted. Terminal revocation requires a fresh authenticated connection.
- This is ownership plumbing only: no service ClipboardBridge installation, consent flow, capture/write lifecycle handoff or operational synchronization claim.


## 2026-10-28: capture provenance and echo-registration boundary

- `input-capture` now exposes a bounded, generation-tagged `ClipboardSample` for the opt-in Windows/macOS text+PNG reader, with one shared worker and one generation-consumption state per reader. `EchoSuppression` is bounded, nonblocking, and matches exact native generation, format, and bytes; it is not automatic echo resolution.
- Existing checked text/PNG emulation APIs return only whether the lifecycle guard admitted the write. They do not return an independently verified native-write generation receipt. Capture therefore must not register echoes by polling afterward or guessing generations; explicit registration is conservative only and requires verified provenance. External applications can still race or replace clipboard contents.
- macOS native NSData may be allocated before size validation, and native clipboard calls can block indefinitely; the implementation bounds admission/concurrency and payload handling but does not provide a hard execution timeout. Focused capture tests pass (36); this is not real Windows/macOS acceptance or service/session/transport wiring.
- Isolated Mac ARM64 prototype validation ran in `/tmp/mousehop-clipboard-macos-AiUF6P` using Rust 1.95.0 native toolchain on macOS 27.0.1. A minimal manifest with objc2 0.6.4, objc2-app-kit 0.3.2 and objc2-foundation 0.3.2 compiled offline; input-event PNG tests 21/21 and macOS prototype tests 7/7 passed, strict targeted Clippy passed. The prototype was never linked into the production manifest, and tests deliberately stop before NSPasteboard access, so no real clipboard was read or changed. The only independent source correction was replacing `NSData::to_vec()` with bounded iterator comparison during readback; this avoids an unnecessary copy while retaining exact-content verification. No service/Windows files, installation, TCC or production process were touched. Native pasteboard runtime, Objective-C exception behavior, hard timeout and dual-device acceptance remain unverified.

## 2026-10-28: consent persistence failure and production wiring boundary

- `SetClipboardDeviceGrant` now fails closed in memory: a failed save never
  restores revoked consent and never creates a new grant. The frontend reports
  failure explicitly. This does NOT durably revoke a previously saved grant:
  if persistence fails, restarting may load the old enabled grant. The regression
  covers all previous/requested boolean combinations, unrelated grants and the
  unchanged durable image; it is not filesystem-failure/end-to-end acceptance.
- Current bridge generation tokens supersede the older terminal-local-generation
  limitation below. No service business-flow wiring was installed. The driver
  lane accessor still clones a shared lane rather than transferring ownership
  once. Outbound connection slots do not retain the actual authenticated remote
  certificate fingerprint (an optional expected pin is not authenticated identity).
  Incoming service notifications expose fingerprint/session but no owned lane.
  Safe handoff needs one-shot lane acquisition, immutable verified fingerprint +
  exact session identity, and replacement/revoke invalidation tests at both owners.
- Capture APIs also prevent a complete lifecycle guarantee: `sample_png` returns
  bytes without generation provenance; text monitor and image reader have separate
  generation/echo state. Required seam: one bounded nonblocking capture source
  returning `{native_generation, format, bytes}`; a shared lifecycle generation
  publication/lock used by revocation, connection replacement and checked OS writes;
  and successful native-write generation/echo registration under that lock for
  both text and PNG. Completion provenance must survive off-executor PNG decode,
  and cancelled tasks must not release admission before native work finishes.
  Polling existing independent readers and guessing generations is unsafe.
- Validation: `cargo fmt --all`; mousehop library 227 passed, one ignored;
  strict all-features library Clippy and `git diff --check` passed. No installation,
  deployment, operational synchronization or native cross-device claim.

## 2026-10-28: isolated clipboard bridge and lifecycle limitation

- `clipboard_session.rs` now includes an isolated `ClipboardBridge` using the
  real lane interface, with codec, permission and retryable backpressure. Its
  bidirectional tests use a bounded mock, not a real DTLS driver. The bridge is
  not installed in the service; there is no operational clipboard sync yet.
- The bridge terminally closes its lane on *any* context change, including a
  normal local clipboard generation change. Since restoration requires a fresh
  authenticated connection, this is a conservative prototype, not a usable
  production clipboard lifecycle. A production design must separate per-transfer
  generation fencing from durable lane lifecycle without accepting stale data.
- `close` cannot retract packets already on the wire; serialize authorization and
  completion checks with OS writes, and do not reuse old lane data after reconnect.
  Peer revocation is effective locally only after it is observed. Strict Clippy
  initially found the unused `clipboard_lane` re-export; it was removed pending
  actual production use. Mousehop library: 220 passed, one ignored; library
  all-features strict Clippy passed. These are not cross-device acceptance.

## 2026-10-28: clipboard DTLS production transport slice

- Supersedes transport-boundary audit below: driver now negotiates bit7 support
  in Hello and routes tag243/version1 through the existing single DTLS reader.
  Both local Hello submission and immutable peer support are required. Bare
  Legacy stays compatible and does not advertise/send these frames. DTLS session
  scope provides connection isolation; permission/session epochs remain a caller
  obligation, not implicit transport authorization.
- crate-visible `clipboard_lane` exposes two-message send/receive queues and
  retryable Backpressure for 1..=65,558-byte frames. Dedicated KCP and one-packet
  low-priority writer lane preserve control/input/motion priority. Malformed bulk
  closes only the clipboard core. DTLS write failure still closes connection.
- New driver test exchanges simultaneous 65,558-byte messages in both directions,
  rejects pre-negotiation submission, injects malformed clipboard traffic and
  confirms reliable input still works. `cargo fmt --all` and focused transport
  tests pass: 78 passed, 131 filtered, 0 failures. These MemoryConn tests exercise
  production driver paths but are not real DTLS or OS clipboard acceptance.
- Remaining: bridge consent lifecycle/sender/reassembler to this lane; fence queued
  data and OS writes on permission revocation; native Windows/macOS image/text
  adapters; safe explicit Legacy wrapper; real DTLS loss/priority acceptance and
  approved dual-endpoint installation. No OS or installation completion claim.


## 2026-10-28: legacy clipboard retired and consent foundation

- Legacy directional clipboard send, receive, forwarding and UI are removed or
  explicitly rejected; old stored direction flags remain readable but never
  grant the new fingerprint-bound, default-off bidirectional device permission.
  New IPC consent snapshots require version 7; the isolated LAN test script's
  identity check was updated accordingly. The new switch is not yet wired to
  an operational transfer, and no clipboard synchronization is currently usable.
- After cleanup, focused mousehop library tests reported 207 passed, one ignored;
  library all-feature strict Clippy and Go tests/vet passed. These are not a
  substitute for final workspace tests or physical Windows/macOS acceptance.

## 2026-10-28: clipboard transport integration boundary audit

- Repaired transient `fnwrite`/`fnnew` syntax damage in clipboard KCP core;
  made its module, Session, Error and full-duplex methods crate-visible for the
  future authenticated driver. Maximum message is now exactly 65,558 bytes,
  aligned with the transfer codec; whole-message `Backpressure` stays retryable.
- Production DTLS driver is deliberately unchanged: no new capability or tag is
  advertised or sent. Its existing single reader is the integration point; do
  not create a second `recv` loop. Explicit Legacy currently returns bare Conn,
  and raw(conn) exposes underlying DTLS, so a wrapper-only prototype is not a
  complete, safe Legacy-compatible integration. A new lane needs negotiated
  immutable support, scoped session identity, bounded separate admission/delivery
  queues, and control/input-priority output with failure isolated from input.
- Core methods alone do NOT perform authentication/consent or fence application
  completion against revoked/replaced permissions. Transport ACK is not an OS
  write receipt. No OS adapter, permission or end-to-end DTLS claim is justified.
- Focused validation after final edits: `cargo fmt --all` and `cargo test -p
  mousehop --lib transport:: --locked` passed (77 tests, 133 filtered, 0 failed).
  Full DTLS clipboard integration remains unfinished; do not call this complete.


## 2026-10-28: native clipboard image foundations

- Opt-in capture/emulation image APIs are not wired to peer synchronization.
  Capture supports Windows PNG/DIB and macOS public.png, not TIFF-only macOS
  content; macOS native build and operation remain unverified. Each direction
  uses one process-wide worker; macOS may allocate NSData before size validation.
- The checked writer requires a caller lifecycle guard held through the native
  write; revocation and published local-generation changes must acquire the same
  lock. External applications can still change the OS clipboard independently.
  Privacy-filtered generations are consumed and not retried on focus change.

## 2026-10-28: clipboard sender foundation

- Added an isolated frame-at-a-time clipboard sender and format-specific receive
  offer validation. Sender frames are retriable without advancing on backpressure;
  callers must advance only once the transport accepts a frame. This does not
  supply consent, cancellation, timeout or OS clipboard integration.
- Wire offers reject empty data and enforce the text 1 MiB / PNG 16 MiB limits;
  the lower-level receiver still accepts empty transfers for its own API.
  `cargo test -p mousehop --locked clipboard_transfer::tests` passed 16 tests.

## 2026-10-28: clipboard PNG codec foundation

- Added `input-event/src/clipboard_png.rs`, gated behind the opt-in
  `mousehop-input-event/clipboard-png` feature. It is pure codec code only:
  no Windows/macOS clipboard API, business-flow wiring, event serialization,
  transport, consent or KCP changes.
- The contract is bounded static PNG <-> tightly packed top-down straight-alpha
  RGBA8. Compressed input/output is capped at 16 MiB, each dimension at 8192,
  pixels at 16,777,216 (64 MiB RGBA), and the png decoder's best-effort internal
  allocation budget is 32 MiB. Text/ICC metadata is ignored and never copied;
  APNG, malformed/truncated/CRC-invalid data and invalid sizes are rejected.
- The feature reuses the existing locked `png 0.18.1` dependency; Cargo.lock is
  synchronized. Future OS adapters must convert native BGRA/row stride/
  premultiplied alpha, bound concurrent synchronous work off the input executor,
  and apply their own clipboard-consent policy.
- `DOC.md` records the path, contract and test command. `cargo test -p
  mousehop-input-event --no-default-features --features clipboard-png --locked`
  passed 21 tests; workspace `cargo clippy --workspace --all-targets
  --all-features --locked -- -D warnings` and `git diff --check` passed. Existing
  default-feature warnings remain. This is not native clipboard or end-to-end
  Windows/macOS acceptance.
- Protocol work in parallel currently defines `MAX_PNG_SIZE=16 MiB`, but its
  `MAX_CHUNK_SIZE=64 KiB` / `MAX_MESSAGE_SIZE` is not the requested ~1 KiB KCP
  message cap. Do not silently change those parallel files; align the KCP
  transport constants before integrating the codec.


## 2026-10-28: isolated clipboard KCP core continuation

- Added socket-free `mousehop/src/transport/clipboard.rs` plus unit tests and
  a private module declaration only; existing driver and protocol files remain
  untouched. Reuses locked kcp 0.6.0. This is NOT production clipboard routing.
- Full-duplex messages are 1..=65,600 bytes (64 KiB chunk plus framing budget),
  MTU 1200, sender budget 128 fragments, application receive queue two messages,
  output queue 260 datagrams. Whole-message send admission returns retryable
  backpressure; malformed data/output overflow/dead-link closes only this core.
- Pitfalls: kcp clamps receive windows to at least 128; account for BOTH receive
  queue and out-of-order buffer. `wait_snd` counts fragments, not messages.
  `flush_ack` clears ACK entries into an MTU-sized staging buffer, not necessarily
  an immediate output packet. Flush ACKs per bounded input to prevent unbounded
  duplicate-ACK accumulation; output overflow is terminal, not silent packet loss.
- Five new tests cover simultaneous maximum messages, data/ACK loss and retry,
  exactly-once delivery, atomic send budget, stalled-consumer bounds/resumption,
  malformed datagrams and output overflow, including surviving real input core.
  `cargo test -p mousehop --lib transport:: --locked`: 77 passed; workspace strict
  all-target/all-feature Clippy and formatting passed. Existing Cargo manifest
  warnings remain. No DTLS multiplexing, priority, consent/epoch negotiation,
  application receipts, OS clipboard behavior or real-device acceptance proved.

## 2026-10-28: clipboard KCP prototype

- Clipboard design is Windows/macOS only, with separate per-peer consent on both ends; old directional permissions must not silently grant broader access. The research notes and draft are in `target/clipboard-design.html` (target is not a durable source document).
- Added core-level two-KCP-session flow-control, loss and failure isolation tests. They do not prove multiplexing over one DTLS connection, priority scheduling, image support, or real-device behavior. Existing Clipboard frames bypass KCP; the driver and authenticated session routing still require changes.
- Clipboard foundations are isolated and not wired to production: `mousehop-proto/src/clipboard_transfer.rs` defines bounded wire messages; `mousehop/src/clipboard_transfer.rs` reassembles chunks with session, permission and local-generation fences; `input-event/src/clipboard_png.rs` provides bounded PNG/RGBA conversion. The 64 KiB codec chunk limit is not the roughly 1 KiB input KCP message limit; an independent bulk transport must segment further or provide a separately bounded framing layer.
- Authorization and session epochs must change on each revocation or replacement; restoring permission must not reuse an old epoch. Receiver completion only returns bytes: final context validation and OS clipboard write must be serialized to avoid stale writes. Neither unit tests nor these foundations establish bidirectional clipboard integration.
- Microsoft MS-RDPECLIP uses a format-specific request/response on paste; this project plans proactive synchronization instead and does not implement RDP compatibility. Do not treat RustDesk image feature requests as evidence of an implemented protocol.

## 2026-10-28: compact vertical layout spacing

- Multi-device top/bottom spacing must not include the spare side drag slot.
  With at most one device per side all four edges use the same 6 DIP gap;
  larger side groups still reserve clearance to prevent corner overlaps.
- Canvas local tile now uses localized "This computer" / "本机", while the
  toolbar retains the actual discovery name. Go tests and vet passed.

## 2026-10-28: README native screenshots

- README now uses only user-captured screenshots/home.png and screenlayout.png.
  These show the current native development app, not the frozen release or
  renderer fixtures. Demo peers have no endpoints; screenshots do not prove
  live connectivity or physical cross-screen acceptance.
- Keep native screenshots distinct from headless renderer test fixtures. An
  earlier automated desktop capture returned a lock screen: do not bypass it;
  use user-provided captures instead. Isolated preview uses dummy input backends
  and separate config/IPC; production app/config must remain untouched.


## 2026-10-28: 1.0.0 prerelease preparation

- User requested build, commit and prerelease with Chinese release notes; no
  installation in this turn. Workspace/internal dependency constraints now1.0.0.
- UI thumbnails intentionally16:10 (not physical desktop proportions), About
  includes version/repository/maintainer/upstream attribution; service status
  moved to fixed bottom-left. Physical backend geometry unchanged.
- Full Windows Rust tests and strict Clippy pass; Go tests/vet include new en/zh
  About/status-position regression. Build snapshot target/package-20261028-154236
  predates only this new test and README/HANDOFF updates; record differences,
  do not claim archive equals complete final tree. Products unchanged by them.
- Mac isolated target must not cp its release binary onto itself: cp exits1;
  resume packaging with set-e and explicit root, never a brittle unguarded slice.


## 2026-10-28: approval lifecycle, packaging and installed update

- Isolated DTLS authorization admission implemented (bounded32,60s,identity/global
  rate limits,nonce checks); denial/expiry/revocation persist approval_paused;
  explicit RetryApproval sends fresh request. Admission never becomes input;
  approval requires a fresh authenticated session. IPC5/MHOP0002 requires updating
  both endpoints. Genuine icon, adaptive layout and localized retry UI included.
- Main session completed full Windows workspace tests, Go test/vet and strict
  all-target/all-feature Clippy; Mac fresh independent-target full Rust tests,
  release build, Go tests/vet passed. Real isolated DTLS/mDNS smoke evidence:
  target/mygo-qa/lan-smoke-20261028-145232-b0e9/result.json. Physical acceptance
  is not implied. Corrected sliding-window rate test and added global identity
  churn coverage; removed unused import and uncalled internal helper.
- Packages: dist/Mousehop-0.17.20-20261028-150535, product display/version no MyGo
  suffix. Windows unsigned, Mac ad-hoc/not notarized. Valid Mac DMG SHA256:
  1c773c0849967949f2ab37f318557111f50913cd8fecfceb3fe2dfc17a9e711a.
- Pitfalls: shared Mac Cargo target reused old IPC/proto artifacts despite new
  source; isolated target fixed and full native tests passed. Never rely only on
  packaging exit status: a sliced script without set-e produced an empty DMG.
  Rejected empty image isolated on Mac; delivered file replaced with verified
  12,749,357-byte DMG, mounted binary hashes/arm64/signature verified. Homebrew
  rsvg-convert requires /opt/homebrew/bin in packaging PATH.
- User authorized both installs. Backups: Windows target/diagnostics/
  install-20261028-150535/{Mousehop-install.backup,Mousehop-config.backup}; Mac
  /Users/alvin/.cache/mousehop/install-20261028-150535/{Mousehop.app.backup,config.backup}.
  Windows installer exit0; both installed binaries match staged payload. Mac
  ditto replacement verified by cmp and codesign. Started normal apps on both;
  Windows frontend responding; Mac frontend88490/daemon88493 observed. No TCC or
  startup setting changed. Permission and physical input acceptance still needed.
- Pre-release publication authorized separately, including temporary disable and
  restoration of four release workflows; release agent verifies source manifest
  and records five non-product cleanup differences. No claim of publication
  until its final URL/restore verification arrives.


## 2026-10-28: responsive screen layout follow-up

- Screen layout now uses the remaining content column instead of a fixed-height
  stage. The measured stage width/height is stored per frame and drives the same
  geometry used for drawing and hit testing; resizing cancels an in-flight drag
  before it can save stale coordinates. Header, pairing/error rows, permissions,
  selector, names, and caption remain outside the geometry fit budget.
- Monitor gap remains 6 DIP, real display aspect ratios and shared-edge
  normalization are preserved, and names remain external with a taller footer
  reservation to prevent two-line overlap. Added resize coverage for minimum and
  expanded windows, error-banner height changes, settled-frame behavior, and
  cancellation without a request. Cached Go 1.27 `go test ./...` and `go vet
  ./...` pass; fixtures were regenerated under
  `target/mygo-qa/layout-spacing-20261028` and inspected with Read. These are
  renderer fixtures, not native-app screenshots. No production process, install,
  package, or commit was performed.
- Known limitation: the headless renderer reports absolute children half a DIP
  inside the bordered stage; tests account for that border. Native OS window
  resize/drag behavior still warrants manual acceptance later.


- Layout gap reduced from 18 to 6 DIP; fixed 200/120 thumbnail caps removed.
  All-direction fit retains true aspect and equal shared edges, with 32-DIP
  total padding. Stage is 360 DIP, 600 for ultrawide-local/portrait-remote.
  Names moved out of tiny faces to a two-column footer with two-line text.
- Cached Go 1.27 full `go test ./...` and `go vet ./...` pass. Added ultrawide
  portrait geometry coverage and named 600-wide render fixtures; Read inspected
  both PNGs in target/mygo-qa/layout-spacing-20261028. Renderer output is not a
  native-app acceptance screenshot. No production process touched or packaging.
- Limitation: stable all-direction fit still limits ultrawide+portrait size;
  the tall canvas can scroll in short windows. Native scrolling/drag acceptance
  remains unverified. Tests with a uniformly 440/600-DIP stage exposed tester
  viewport clipping/scroll coordinate differences, so do not simply increase
  the stage universally. Current portrait regression uses a 760-high viewport;
  minimum-height native portrait drag needs follow-up before final acceptance.


## 2026-10-28: screen layout material expression

- Screen tiles now render as inset monitor faces, with subtle rim highlights,
  lower hardware marks and grounded shadows. Neutral dot grid replaces blue
  grid. Local/remote identity, rectangles, aspect ratio and drag hit bounds stay
  unchanged. Remote retains blue rim/grab cursor; canvas footer reflects pending
  save/offline state. No new protocol requests or background polling.
- Go tests pass after final footer; vet passed before footer addition. Inspected
  renderer Screen-layout-680.png in target/mygo-qa/layout-material-20261028
  before footer addition; not native visual acceptance. Updated isolated preview
  mousehop-mygo-preview-layout.exe launched --no-daemon, PID 78284. Only previous
  preview stopped; installed processes and config untouched.

## 2026-10-28: tabs below caption controls

- User explicitly requests tabs NOT aligned with minimize/maximize; toolbar now
  has a separate 42-DIP navigation row below a 36-DIP native caption/drag area.
  Tests assert navigation is below native caption height and still clickable.
- Added shared quiet/accent button treatment on main views, neutral layout
  canvas and subdued Configure-open state. Full Go tests and vet pass.
- Renderer fixtures: target/mygo-qa/material-tabs-20261028. Isolated preview
  executable mousehop-mygo-preview-next.exe is launched with --no-daemon and
  build-time applicationName=Mousehop-Preview to avoid the installed single-
  instance lock. Only the prior verified preview PID was stopped; production
  app/daemon remain untouched. Native appearance is user acceptance, not yet
  automated screenshot evidence. Animated sliding selection remains unfinished.

## 2026-10-28: integrated graphite window material, first pass

- User requests material quality, not screenshot layout/size: neutral graphite,
  integrated titlebar, Ballmac AppWindow/SegmentedControl reference. Native MyGo
  cannot directly run the reference React components; native styling is adapted.
- Frontend now uses TitleBarHidden with a 50-DIP content toolbar, native caption
  buttons retained. c.TitleBar() reserves left/right OS button space; DragWindow
  preserves native dragging. Do not replace close-to-tray behavior or daemon code.
- Navigation uses SegmentedBase radio semantics, recessed track and raised inset-
  highlighted selection; neutral opaque surfaces replace the previous blue cast.
  Cards use half-DIP borders and subtle static shadows. No backdrop blur/polling.
- Go tests and vet pass, including native-caption space and navigation click
  regression at minimum width. Default PATH Go is too old for -C; use cached
  golang.org/toolchain go1.27 executable as build-mygo.ps1 does.
- Renderer fixtures: target/mygo-qa/material-20261028; Devices-680.png inspected.
  These are NOT native screenshots. Native window dragging/caption/resize and
  macOS appearance remain unverified. Button-specific polish and animated pill
  remain follow-up; this is not final visual/native acceptance. No install,
  restart, commit, push, or production configuration mutation.

## 2026-10-26: wired input stalls and intermittent Mac window close failure

- Read this file before continuing this incident. Source HEAD inspected:
  `39e055874b6f9522a957915d3c21d2aac254db2a` (v0.17.17).
- Evidence lives in `target/diagnostics/wired-stall-20261026-1716/`.
  This directory is ignored and may not exist in a fresh checkout.
- The user reports Windows-to-Mac movement freezing for 2-5 seconds on Ethernet,
  not noticed on Wi-Fi. They also confirmed the intermittent Mac close-button
  failure occurs with the local trackpad. Treat these as two separate issues.
- Actual Mac route to Windows 192.168.180.134 used en6 (USB Ethernet). A short
  20-packet ICMP sample had 0% loss and 0.914 ms average RTT. That sample is not
  proof that all incident-time packets were delivered.
- Mac 17:15:01 +08 triggered input recovery, 17:15:02 closed for PeerTimeout;
  Windows 17:15:04 reported peer_age=4706ms with pending=0, consuming=0. Additional
  incidents occurred, many near 30-second periodic boundaries. Do not assume
  every freeze has one cause or label this as ordinary Wi-Fi jitter.
- Windows uses a current-thread Tokio runtime (`mousehop/src/main.rs:178`).
  Periodic discovery/network refresh and listener reconciliation synchronously
  call netdev interface/gateway queries on that executor. netdev 0.43.0's Windows
  interface enumeration calls blocking SendARP when obtaining gateway MACs.
- A separate helper using the exact netdev version measured 30-1168 ms per API
  invocation. The first round of get_interfaces + get_default_gateway +
  get_default_interface took about 3.5 seconds. Repeated queries were variable,
  so do not describe a fixed 1.16-second latency or extrapolate an exact total
  for the live daemon. The helper can also warm OS neighbor caches.
- Read-only Enumerate IPC polling at 4 Hz observed Windows daemon response gaps
  of 1846 ms (17:23:11.504 +08) and 2695 ms (17:23:42.393 +08), even while the
  client snapshot reported Failed: peer activity timed out. This supports a
  local event-loop scheduling/service stall, not merely waiting for KCP packets.
  The exact live blocking stack was not captured; attribution to netdev is a
  strong source-backed hypothesis, not a complete incident trace.
- Mac GUI sample at 17:16:19 showed normal main-loop event waiting, not a captured
  hang. It does not rule out an intermittent hang. No close failure was reproduced
  under instrumentation. Mac close-request is intended to hide, not quit.
- No app restart, install, network/config/TCC changes or source repair was made.
  Only diagnostic helpers, logs and documentation were added. Do not claim a fix.
- Next: isolate/coalesce slow network discovery on a bounded worker; do not await
  slow results in a way that blocks Service input dispatch, create unbounded
  workers, discard listeners on failed snapshots, or mask stalls by increasing
  input timeouts. Validate executor latency and real wired input over multiple
  refresh periods. Independently capture a GUI sample and window/modal state
  during an actual trackpad close failure before selecting a GUI repair.

## 2026-10-26: repair implemented, target-machine acceptance pending

- Slow network interface/gateway discovery now runs through a coalescing
  `BackgroundRefresh` on Tokio's blocking pool. The Service select loop receives
  completed snapshots instead of awaiting OS discovery in its dispatch branch.
  Network fingerprints and mDNS share one enumeration; unchanged advertisements
  are not republished. No KCP timeout, protocol, or input recovery limits changed.
- Network-policy changes wait for the fresh snapshot in a bounded queue (32).
  Query failure retains cached state and rejects queued changes rather than
  persisting locks against a stale network.
- Listener enumeration is also off-executor. Failed enumeration retains live
  listeners. Up/Down revisions invalidate older snapshots; port changes reuse
  the same enumeration worker, avoiding overlapping queries in that supervisor.
- GTK request writes now use a dedicated thread and a 64-entry nonblocking FIFO.
  Full queues report an error. Writes have a one-second socket timeout; partial
  failures shut down the IPC stream to wake the reader's reconnect path. Old
  queued requests are discarded when the writer is replaced.
- Mac activation-policy changes are deferred out of visibility callbacks and
  close/visibility logs were added. These remove identifiable blocking and
  reentrancy risks, but the trackpad close failure was NOT reproduced under
  instrumentation. Do not claim its root cause or UI acceptance is confirmed.
- Windows final workspace tests: 314 passed, 0 failed, 3 ignored, 1 filtered.
  Mac arm64: 354 passed, 0 failed, 2 ignored, 1 filtered. The ignored counts
  include the new live scheduling probe, which separately passed on each OS.
  The filtered test is `latency::tests::unroutable_address_times_out_as_unreachable`:
  it failed in the first Windows run under the existing network environment.
  Do not report an unqualified full-suite pass.
- Both platforms passed `cargo clippy --workspace --all-targets --all-features
  --locked`; formatting and diff checks passed. Existing Cargo feature warnings
  and Mac native archive-tool warnings remain.
- Three live OS query probes: Windows 14.9-24.7 ms query duration, max executor
  tick gap 14.7 ms; Mac 4.6-51.2 ms query duration, max gap 11.1 ms. These are
  short, potentially cache-warmed probes, NOT incident reproduction or a
  long-running dual-machine acceptance test. The injected blocked-query test
  independently covers dispatch progress and query coalescing/cancellation.
- Logs: `target/diagnostics/wired-fix-*.log`. User-facing repair report:
  `docs/diagnostics/2026-10-26-wired-stall-fix.html`.
- Mac native validation used the isolated worktree
  `/Users/alvin/.cache/mousehop/wired-fix-20261026` at the same base HEAD plus
  this patch. No installed app was replaced or restarted; no TCC/network settings
  changed, commit created, or release published.
- Remaining acceptance: install approved test builds, exercise wired crossing
  and motion across multiple 30-second refresh periods, stop/reverse accurately,
  and repeatedly close/reopen using the local Mac trackpad (with/without modals).
  Capture a GUI sample plus close/visibility logs during any remaining failure.

## 2026-10-26: v0.17.18 release preparation

- User authorized commit, push, packaging, and publication after the repair.
  Publish to `alvin-7/mousehop`, remote `alvin`, branch `main`, not upstream
  `origin` (`jondkinney/mousehop`). New tag is `v0.17.18`.
- Build Windows x64 installer locally and macOS Apple Silicon DMG natively from
  the exact clean release commit. Verify version, commit, archive integrity,
  signature/dependency closure and upload hashes. Copy the ARM64 DMG to Mac
  Downloads; do not silently replace the installed app or alter TCC/network.
- GitHub CLI is absent on both endpoints. The existing credential-manager-backed
  helper `target/release-0.17.17/github-release.mjs` can publish through the API
  without printing credentials. Local API/package evidence goes under ignored
  `target/release-0.17.18/` and final artifacts under `dist/release-0.17.18/`.
- Dispatch cloud builds against the new tag, not a moving main branch. Publish
  only assets actually verified; record any unavailable platforms explicitly.
- Release notes must retain the live-acceptance limitation and distinguish
  unsigned Windows / ad-hoc Mac signatures from notarized releases. Recheck
  native bundle load-command minimum OS versions rather than assuming support.

## 2026-10-26: pre-publication deferred-request lifecycle fix

- Review caught an asynchronous lifecycle regression before publication:
  `ClientManager` reuses Slab handles, so a queued policy request could outlive
  a deleted client and apply to its replacement after the network query ended.
- Reject policy requests for nonexistent clients before enqueueing, and remove
  matching deferred requests in `remove_client`, covering both explicit deletion
  and configuration reload. Regression exercises real Slab handle reuse and
  verifies requests for other clients retain their order.
- `v0.17.18` was pushed but never published. Its draft release was deleted and
  cloud packaging run 40304289490 cancelled. Preserve the tag; publish `v0.17.19`
  from a new commit rather than rewriting a pushed tag or mislabeling binaries.
- Final release artifacts/evidence use `dist/release-0.17.19/` and
  `target/release-0.17.19/`; the earlier 0.17.18 outputs are not release assets.
- Version 0.17.19 validation passed: Windows 315 workspace tests (3 ignored,
  1 filtered), Mac arm64 355 (2 ignored, 1 filtered); both all-target/all-feature
  Clippy runs passed. The skipped latency test and pending live acceptance are
  unchanged. The new handle-reuse regression is included in these counts.

## 2026-10-27: right-Shift chord leaves macOS Shift held (investigating)

- User reports left Shift + grave produces tilde and releases normally, but
  right Shift + grave leaves Shift held after release, following Win-to-Mac
  crossing. Both installed endpoints were verified as 0.17.19; Mac reports
  tag v0.17.19 / dfb532bc and Windows executable version resources report .19.
- Read-only native snapshots at 03:31:20Z and 03:32:59Z: Windows async state
  shows both Shift keys released. Mac HID and combined state both report
  flags 0x20020002, left Shift (56) true and right Shift (60) false.
  These are sequential endpoint samples, not atomic cross-machine captures.
  They prove the sampled macOS left-Shift state, not that a right key was
  mistranslated or that this combination alone created that left-side state.
- Evidence: `target/diagnostics/right-shift-20261027/` (two native snapshots
  per OS and preserved daemon logs); reader is `shift-state-readonly.py` in
  the parent diagnostic directory. No input was injected or app restarted.
- Source maps Windows 0x36 / 0xE036 to evdev right Shift 54; the Mac planner
  maps it to right Shift keycode 0x3c and side bit 0x4. No simple left/right
  constant error found. `locally_held_flags` deliberately preserves an
  observed opposite side, so unexplained left-side residue needs event-level
  attribution before changing ownership or clearing locally held keys.
- Existing info-level logs do not contain capture/injection modifier traces.
  Next: obtain approval for a diagnostic restart of the same installed apps,
  collect filtered modifier capture and Mac injection/OS-state transitions,
  and compare left vs right Shift/grave sequences. Do not blindly clear both
  Shift keys, claim a fix, publish, or change TCC permissions on this evidence.

### Diagnostic restart authorized and completed

- User approved the diagnostic restart on 2026-10-27. Preserved pre-restart
  native state and daemon logs in the same evidence directory first.
- Relaunched the same installed v0.17.19 binaries. Executable and config
  SHA-256 values before/after match on both OSs; no installation or TCC change.
- Mac daemon 17009 exited on SIGINT before GUI 17008 was terminated. The app
  was reopened with LaunchServices; new GUI/daemon are 45813/45814. Collector
  PID 45810, directory `/Users/alvin/Library/Logs/Mousehop/right-shift-diagnostic-20261027/`.
  Both processes inherit `info,mousehop::keyboard=debug`; stderr points to the
  collector FIFO. Mac HID/combined Shift and other modifiers are initially clear.
- Windows old daemon/GUI 51848/62900 were stopped after exact-path checks;
  new GUI/daemon 62032/71584, collector PID 40240. Filter adds
  `mousehop::capture=trace`. Logs: evidence directory `windows-live/modifiers.jsonl`.
- Both collector privacy self-tests passed. Keep only modifier captures,
  snapshots/injections/release confirmation, selected lifecycle/warnings, and
  native modifier-state changes (50 ms polling). Ordinary keys, pointer and
  clipboard traces are discarded in memory. Heartbeats are every 30 seconds.
- Collectors remain active awaiting the user's left/right Shift comparison.
  Do not stop them or clear the stuck state before copying the reproduction.
  No product code fix, commit, packaging, or publication is authorized by this
  narrow diagnostic restart. To restore normal logging later, exit the diagnostic
  apps, create `stop` in each collector directory, then relaunch normally.

### Reproduction captured; shared modifier-readback repair implemented

- User reported reproduction after the diagnostic restart, then explicitly
  requested checking and fixing other keys too. New source edits are scoped
  to all eight modifier sides; no installed binary, configuration, or TCC
  settings were changed. The diagnostic apps/collectors remain running.
- Preserved `windows-repro1.jsonl`, `macos-repro1.jsonl` and native snapshots.
  Windows captured right Shift (evdev 54) down and up. Mac submitted right
  Shift keycode 0x3c with flags 0x20004, then initially released with flags 0.
- The decisive Mac trace at 03:41:28Z: native flags name right Shift while
  key-state bitmap names left Shift. The release retry publishes 0x20002
  through BOTH IOHID and CGEvent, then reports right-side release confirmed.
  The OS remains at 0x20020002 with left Shift held. Left-Shift comparison
  had cleared normally first; the reproduction started from clear state.
- Root cause is `MacKeySink::modifier_flags`, added by 39e0558: it ORed
  `flags_for(side)` for each true CGEventSourceKeyState bit. Synthetic modifier
  bitmap aliases are not reliable side ownership; the fabricated left bit
  was mistaken for local input and preserved/re-posted by the release retry.
  This is not an observed missing Windows release or a transport-loss finding.
- New `observed_modifier_flags` preserves native device-side flags and uses
  bitmap evidence ONLY for aggregate kind bits. Bitmap-only stale state still
  blocks premature release confirmation but cannot invent an opposite side.
  Shared by left/right Shift, Control, Option and Command; no blanket reset.
  Existing same-side simultaneous local/remote ownership ambiguity remains.
- Seven added regressions cover all sides, 65,536 flags/bitmap combinations,
  both-source unions, 56 distinct remote/local side pairs, ordinary key inside
  a chord, and all-eight-side cleanup. Old algorithm fails the exact right-Shift
  regression (CG release flags 0x20002 instead of 0); repaired algorithm passes.
  Existing Caps Lock, ordinary-key, repeat, release-failure and retry tests pass.
- Workspace tests: Windows 322 passed / 3 ignored / 1 filtered; native Mac ARM64
  362 passed / 2 ignored / 1 filtered. Filter is the same environment-sensitive
  unroutable-address latency test as the prior release. Both platforms passed
  all-target/all-feature Clippy. Formatting and diff checks passed. Existing
  Cargo/native archive-tool warnings remain.
- Initial Windows workspace validation lacked GTK pkg-config environment.
  Re-ran with `target/kcp-tools/gtk-2026.8.0` bin, lib, include and pkgconfig;
  do not install GTK again or report that prerequisite failure as a code failure.
- Mac tests run in `/Users/alvin/.cache/mousehop/right-shift-fix-20261027`, exact
  tagged source plus the two changed Rust files; their SHA-256 values match this
  checkout. Reused the prior validation target cache, never the installed app.
- Report: `docs/diagnostics/2026-10-27-right-shift-residue.html`; design details
  updated in DOC.md. Physical-key acceptance of the repaired build is pending;
  other modifier sides have shared-code and regression coverage, not a claim
  that every side was physically reproduced. No commit/package/release made.

### Requested Windows x64 and Mac Apple Silicon packages built

- User requested packaging only. Bumped workspace/lock versions to 0.17.20
  without dependency upgrades, commit, push, tag, publication or installation.
  Includes the shared modifier-readback repair above; physical acceptance pending.
- Delivery directory: `dist/build-0.17.20/`. Windows installer is
  `Mousehop-0.17.20-x64-Setup.exe` (47,020,609 bytes), SHA-256
  `e6e2b766790a427bd7c18756bf73171d0a01c694d11bd422cf4ea453a7857d78`.
- Apple Silicon DMG is `Mousehop-0.17.20-macOS-arm64.dmg` (15,791,330 bytes),
  SHA-256 `4afb7a48ed6a60d8af9f410dbf6d3bed884446c4c4154a3853b7437b36316b7f`.
  Identical copy delivered to `/Users/alvin/Downloads/`; no Intel package built.
- Mac is a native local build with Homebrew dependencies requiring macOS 27.0,
  NOT the prior macOS-15-compatible cloud runtime. Set LSMinimumSystemVersion
  to 27.0, then re-sign ad-hoc. Not notarized; Windows installer is unsigned.
- Final checks: both CLI version/help smoke tests pass; Windows 78 PE binaries
  are x64 with closed static imports, Mac 43 Mach-O binaries are ARM64 with
  closed dependencies. DMG integrity/read-only mount, resources/plist, and strict
  deep codesign pass. Mac maximum dependency minimum OS verified as 27.0.
- Both platforms build from the same fixed Rust files and version/lock hashes.
  Exact source snapshot, SHA256SUMS and provenance are in the delivery directory;
  logs/audit scripts in `target/release-0.17.20/`. Windows embeds base .19 tag/
  commit; Mac archive has no Git metadata. Neither is a clean tagged release.
- Mac source/build: `/Users/alvin/.cache/mousehop/right-shift-package-0.17.20-20261027`;
  reused .19 target cache for compilation only, bundled into the isolated new
  source target using cargo-bundle --binary-path. Installed executable hashes
  on both endpoints remain unchanged; diagnostic apps/collectors not restarted.
- Packaging pitfalls: git-archived shell scripts have CRLF; execute LF-normalized
  temporary copies of makeicns.sh/copy-macos-dylib.sh. PowerShell GUI-executable
  redirection gave empty logs: final Windows CLI smoke used Python subprocess
  pipes with explicit UTF-8 (default GBK cannot decode the help text).

### Repair confirmed by user; v0.17.20 committed and released

- User confirmed the repair and authorized commit/push/publication for Windows
  x64 and Mac Apple Silicon only. Re-ran 0.17.20 validation: Windows 322 passed,
  3 ignored, 1 filtered; Mac ARM64 362 passed, 2 ignored, 1 filtered. Both
  workspace/all-target/all-feature Clippy runs and formatting passed. Same
  environment-sensitive latency test filtered; other modifier combinations
  remain regression coverage, not exhaustive physical acceptance.
- Commit `29a735cfee86a3551ed5d40aec06268bf23e32b9`, annotated `v0.17.20`,
  pushed atomically to alvin/main plus tag. Included only Cargo.toml/lock,
  two Mac backend Rust files, DOC.md and CHANGELOG.md. HANDOFF and the local
  diagnostic HTML were explicitly excluded and remain local worktree changes.
- Both release packages were rebuilt from clean exact-tag source, not uploaded
  from the earlier uncommitted test builds. CLI version embeds v0.17.20 and
  29a735cf on both platforms. Each platform's 177 Rust/manifest/lock source
  files match the validated worktree after line-ending normalization.
- Release: https://github.com/alvin-7/mousehop/releases/tag/v0.17.20
  Published stable, release ID 425072431. Exactly two binary assets plus
  SHA256SUMS.txt and BUILD-PROVENANCE.json; no Linux, Flatpak or Intel Mac.
- Official Windows installer: 47,025,883 bytes; SHA-256
  `45a311b62cf3fa576261fe11cca4e33a467e89aa447746e556fd7ea6c4b316b6`.
  Official ARM64 DMG: 15,791,569 bytes; SHA-256
  `dd4dd09203e441a23f70c3ff4a461cd7a2a66ffc9f7051839b617ea9fa58ac41`.
  Files in `dist/release-0.17.20/`; logs/audits in `target/release-0.17.20/`.
- Final audits passed: 78 x64 PE binaries/static import closure; 43 ARM64
  Mach-O binaries/dependency closure, DMG integrity/read-only mount, bundle
  resources/plist and strict deep codesign. Mac minimum OS is explicitly 27.0
  in plist, dependency audit, changelog and release notes. Mac ad-hoc signed,
  not notarized; Windows unsigned. Do not present this as macOS-15-compatible.
- Published DMG copied to `/Users/alvin/Downloads/Mousehop-0.17.20-macOS-arm64.dmg`;
  previous local test DMG preserved as `Mousehop-0.17.20-local-test-macOS-arm64.dmg`.
  Copied hash verified. No installed app, process or TCC change performed.
- To prevent release-triggered extra-platform assets for this explicit limited
  manual release, temporarily disabled the active Release/Flatpak/AUR workflows
  around publication and restored all three active states in finally. Evidence:
  `publication-evidence.json`; restore_failures is empty. No workflow code changed.
- Final public verification: release is non-draft/non-prerelease and latest is
  v0.17.20; all four assets downloaded without authorization and their full
  SHA-256 values match local files and GitHub asset digests. Evidence:
  `public-release-verification.json`. No release-event build runs were triggered.
  The unauthenticated latest-metadata lookup hit HTTP 403 after the downloads;
  rechecked metadata with the existing credential, reusing verified downloaded
  files rather than downloading all packages again. Never log the credential.

### Chinese standalone release notes and fork-specific README

- User plans to retain only one Release and requested a standalone Chinese
  v0.17.20 description, then also requested README describe this modified fork.
  Updated the existing Release body only; did NOT delete any other releases.
- README now explains lan-mouse -> jondkinney/mousehop -> alvin-7/mousehop,
  inherited capabilities versus this branch's KCP/UDP, cleanup, Mac keyboard/
  trackpad, network/GUI responsiveness, packaging and diagnostic modifications.
  Corrected upstream download links, package format and signing assumptions;
  retained attribution/GPL, configuration, CLI, validation and known limitations.
- README-only commit `86c0c2f4c6016c1a5055f56681a89f73b984567c` pushed to
  alvin/main. v0.17.20 remains at 29a735c; binary assets, asset IDs/digests,
  release status and publication time are unchanged. No rebuild/new version.
- Release Chinese body is self-contained: current features, cumulative fork
  changes, specific Shift repair, downloads, setup, upgrades and test limits.
  Explicitly retains macOS 27.0+, Windows unsigned and Mac ad-hoc/not notarized.
- Verified live Release body equals local notes and published README content
  equals local README. Markdown fences/local links/images and TOML sample pass.
  Preserved experimental-reliable-input-over-dtls anchor for DOC.md's old link.
  Documentation-only change; no runtime tests rerun. Evidence is under
  `target/release-0.17.20/chinese-doc-*.json` plus before/after release snapshots.

## Tool pitfalls

- Use PowerShell quoting end-to-end. Bash-style escaped single quotes embedded
  in a PowerShell SSH command cause parse errors. Build the remote command in a
  PowerShell variable, or transfer a script.
- context-mode tools were not callable during this continuation (unsupported
  call). Keep native command output bounded and summarize data in code.
- Mac WindowServer metadata from an SSH session is not a substitute for a visual
  close-button reproduction. Missing on-screen/title fields do not prove that a
  window is hidden, visible, modal, or stuck.
- Prefer `C:\Windows\System32\OpenSSH\ssh.exe` and `scp.exe` here. PATH can
  select SSHFS-Win's executables, which produced inconsistent host resolution
  and host-key failures. Do not disable host-key checking to work around this.
- On macOS, setting a read timeout on an already shut-down Unix socket may
  fail with EINVAL. Set test read timeouts before injecting the failed write.

## 2026-10-27 - MyGo UI migration (in progress, not a release)

- Branch `mygo` is based on README commit `86c0c2f`. User explicitly requires
  deleting the old UI, not keeping GTK as a fallback. Removed mousehop-gtk and
  Rust GTK dependencies; sibling Go/MyGo executable now owns the native UI and
  owned-daemon supervision. Reference pins/license notices are in mousehop-mygo.
- User's approval UX: same email only groups LAN discovery. A clicks Connect;
  B sees A's name/advertised addresses and Allow/Decline, never a fingerprint
  string. New requests surface the window, with duplicate focus suppressed.
  Allow persists the actual rejected DTLS certificate, not just an mDNS claim.
  Discovery-selected outgoing peers pin the target certificate across restarts.
  Names/advertised addresses are unverified metadata. Legacy/manual unpinned
  clients preserve their old behavior; reverse direction is not auto-approved.
- Email hash is a guessable LAN namespace, not a credential. No cloud account,
  email verification or implicit clipboard permission. Pending/nearby/rejection
  queues and UI IPC queues are bounded; slow frontend writes run separately
  from input dispatch, use write_all, and disconnect on overflow.
- Corner anchors map cross-axis backend coordinate units and refuse crossings
  outside the overlapping edge. Source geometry comes from Begin's snapshot;
  inverse mapping also applies to return warp. Windows now supplies cached
  geometry and accepts release warp targets. Real movement/return/negative
  origin acceptance is STILL pending. UI uses authoritative daemon geometry,
  not MyGo DIP bounds or invented dimensions. Missing sizes are placeholders.
- Current Windows Rust validation: 321 passed, 2 ignored, 1 filtered (existing
  unroutable_address_times_out_as_unreachable); all-target/all-feature Clippy
  and fmt pass. Cargo's existing workspace default-features warnings remain.
  Go tests/vet pass, including native-memory rendering at 1100x760/820x600,
  unequal-screen anchors, dragging, hidden identities, allow/decline and queue
  generation isolation. PNGs in target/mygo-qa are fixtures, not real sessions.
- Durable scripts/test-mygo-lan.py passes same-host real mDNS+DTLS with two
  dummy daemons: separate config/cert/UDP/IPC, actual spawned PID and native UI
  IPC revision verified BEFORE any mutation. Partial startup is cleaned up;
  only owned children can be shut down. Latest evidence is in
  target/mygo-qa/lan-smoke-*/result.json. This is NOT physical Windows/Mac QA.
- Additional isolation pitfall: different ports/configs are not enough for
  mDNS. Tests on the same host need unique service AND host-record names, or
  they collide with each other / an installed app's hostname. The smoke now
  sets MOUSEHOP_DISCOVERY_NAME to an independent random DNS label per child.
- MyGo waits for ServiceIdentity ipc_version=1 before sending UI mutations;
  an installed older daemon must not be silently changed or shut down just
  because it occupies the default IPC endpoint. An incompatible attached
  daemon produces an instruction to quit the other instance, not an upgrade.
- Windows paired dev build passes via scripts/build-mygo.ps1. CI/release,
  Windows installer, Mac bundle, Flatpak/AUR/Nix recipes now stage MyGo+Rust,
  not old GTK UI resources. Mac main bundle executable is mousehop-mygo;
  scripts/package-macos.sh rejects non-system dylibs and derives minimum OS
  from BOTH Mach-O executables. Removed old GTK dylib-copy/cargo-bundle path.
  Actionlint and PowerShell/shell syntax pass; actual cross-platform packages
  and native Nix vendorHash still need verification. Nix requires Go 1.27.3+.
- System Go 1.18.4 is too old. Working toolchain is the cached go1.27.3 under
  ~/go/pkg/mod/golang.org/toolchain@v0.0.1-go1.27.3.windows-amd64/bin. Windows
  build helper can select it. Python defaults to GBK: always specify UTF-8 for
  repository text. PowerShell native GUI redirection can lose stdout; use
  Python subprocess pipes when version output itself must be checked.
- Still incomplete: release shortcut/threshold editing, complete host/IP/port/
  hooks and RTT/network pin UI, About/version view, macOS permission onboarding,
  native tray/lifecycle/Chinese IME tests, dual-machine movement/key cleanup and
  idle CPU/memory. See docs/mygo-ui-progress.html. Do not mark the goal complete
  or publish this branch as a replacement release yet.
- Installed binaries/config/TCC and existing diagnostic collectors are untouched.
  No commit/push/publication performed. Preexisting HANDOFF edits and the local
  right-shift diagnostic HTML remain user work; do not include them implicitly
  in a later business commit.

### Resident lifecycle requirement (Windows and macOS)

- User requires both Windows taskbar + notification area, and macOS Dock +
  menu bar. Windows close now MINIMIZES, not Hide (Hide removes the taskbar
  button). macOS keeps ActivationPolicyRegular and LSUIElement=False; closing
  hides only the page. Open restores minimized windows before showing/focusing.
- Resident menu: Open Mousehop, opt-in Open at login, Restart Mousehop, Quit.
  Stable tray ID retains OS placement preferences. Mac primary click opens
  the menu; Windows primary click opens the page and secondary click the menu.
  A Dock menu exposes the same options. Login launches park the page quietly;
  tray creation failure surfaces the page and preserves close-to-quit fallback.
- Startup state comes from the OS, not a second config flag. The menu refreshes
  on activation and before tray popup; failed registration reports the OS error
  and re-reads the actual state. No startup setting was enabled on this machine.
  Mac requires a bundle; SMAppService can require user System Settings approval.
- MyGo Relaunch starts its replacement AFTER OnQuit but BEFORE App.Run returns.
  A deferred daemon.stop alone races the replacement. Cleanup is now registered
  in OnQuit BEFORE RequestSingleInstanceLock registers lock release. An attached
  independent daemon is never stopped; only the supervised child is shut down.
- Go tests/vet and Windows frontend build pass. New adapter tests cover the
  window/menu policy, external startup-state changes and registration failures;
  subprocess tests cover capability shutdown, bounded kill and non-owned attach.
  Go race detector was not run: CGO is disabled and no gcc is available on PATH.
- Latest Go JSON test run: 39 passed including subtests, 0 failed, evidence in
  target/mygo-qa/resident-go-tests.jsonl. darwin/arm64 frontend cross-compiles
  with CGO disabled to target/debug/mousehop-mygo-darwin-arm64; this does NOT
  validate the Mac Rust backend, bundle, Dock/login behavior or runtime.
  Workspace Rust formatting/Clippy and macOS package shell syntax also pass.
- Native Windows QA hit a lock screen (initial activation failed; re-observation
  showed the lock screen). STOPPED GUI actions; no native tray/restart acceptance
  is claimed. Isolated no-daemon PID 51372 was path/argument verified and cleaned
  up; installed PIDs 17988/75732 were untouched. Evidence:
  target/mygo-qa/resident-20261027-231612/result.json. Unlock the desktop before
  resuming Computer Use. macOS Dock/menu/login runtime still needs a Mac.

### User correction: notification/menu-bar only, NOT taskbar/Dock

- This supersedes the resident-lifecycle interpretation immediately above.
  User explicitly means the Windows bottom-right notification-area icon and
  the macOS top menu-bar icon, NOT minimized app/taskbar buttons or Dock icons.
- Closing now hides the page on BOTH platforms, never minimizes it. Windows
  thereby removes the taskbar button while the tray stays. macOS uses
  ActivationPolicyAccessory and LSUIElement=True, with no custom Dock menu.
  Keep manual-minimize recovery when explicitly opening the page; that is not
  the close-to-background behavior. Login launches remain hidden.
- Resident menu options and safe owned-daemon Restart/Quit remain unchanged.
  Updated regression tests assert Hide (not Minimize), macOS accessory policy
  and restoring a manually minimized page. README and HTML now state the
  corrected behavior. Prior native probe is not evidence for this correction;
  locked-desktop native acceptance and Mac runtime acceptance remain pending.
- Corrected code passes Go test/vet, Windows frontend build, darwin/arm64
  frontend cross-compilation, and macOS package shell syntax checks. This is
  compile/test evidence only, not native tray/menu-bar/login acceptance.

### Drag-only layout with magnetic corner snapping

- User requires arranging screens by dragging, NOT direction/corner buttons.
  Removed those controls. The local screen stays fixed; the selected remote
  screen follows the pointer and snaps within 18 DIPs of an edge/corner slot.
  A ghost target, alignment guide, corner dot and temporary corner name expose
  the intended drop. A dropdown chooses the peer only when several exist.
- Use a fixed canvas input handler, not MyGo Draggable's projected Snap point:
  Draggable projects fling velocity at release, which could choose a different
  edge/corner from the preview the user actually saw. This implementation uses
  the actual pointer and sends nothing while moving. Escape, blur, page change,
  disconnect or release outside the canvas cancels; a click never saves.
- A drop sends ONE UpdateScreenPlacement(handle, position, alignment) request.
  ClientManager updates both fields under one borrow before capture recreation
  or State events. Keeping two separate requests risks an intermediate wrong
  landing map and a half-persisted layout. Native IPC revision bumped 1 -> 2;
  MyGo rejects older preview daemons. DTLS/KCP peer serialization is unchanged.
- Pending preview waits for matching authoritative State; send failure or a
  three-second confirmation timeout reverts. One UI wake handles the timeout,
  not an always-running animation/poll loop. Test coordinates must be relative
  to the canvas: an error banner legitimately moves the canvas on the page.
  Float comparisons for scaled corner geometry need a tolerance.
- Current validation: Go test 65 passed including subtests; Go vet, paired
  Windows dev build, darwin/arm64 frontend cross-compile, workspace Rust fmt
  and all-target/all-feature Clippy pass. Rust tests: 323 passed, 2 ignored,
  1 filtered (same environment-sensitive network test as before).
- Evidence: target/mygo-qa/drag-snap-20261027-234951/go-tests.jsonl and fixture
  Screen-layout-snapping.png (visually inspected; NOT a native app screenshot).
  Layout tests exercise all 8 side/corner targets at 820 and 1100 widths,
  magnetic movement, fixed local screen, no motion writes, atomic drop, ack,
  cancellation, queue failure and timeout. Rust tests cover serialization and
  active/inactive placement semantics.
- Extended real mDNS/DTLS smoke passes the new atomic request and persisted
  position/alignment pair. Evidence: target/mygo-qa/lan-smoke-20261027-235129-cb04.
  Both isolated dummy daemons cleaned up; installed PIDs 17988/75732 untouched.
  Still no physical Windows/Mac drag/input or tray/menu-bar/login acceptance.
- Release-shortcut/threshold migration was interrupted by this higher-priority
  user clarification before any release-editor code was changed; it is STILL
  pending, not completed by the layout work.

### 2026-10-28: MyGo release-shortcut and fallback settings

- Completes the release-shortcut/threshold gap recorded above. Settings now
  offers eight independent left/right modifier checkboxes and a searchable
  trigger picker. Modifier-only and single-trigger chords work; existing rare
  or multi-trigger keys are retained unless the user replaces that trigger.
  No aggregate-modifier recorder silently guesses left instead of right.
- Use default sends SetReleaseBind([]), NOT null or a disable operation. The
  daemon echoes its canonical default. Empty custom chords cannot be applied.
  Physical sides are preserved, including selecting both sides of a modifier.
  Picker enum names are checked against input-event/src/scancode.rs: spellings
  include KeyRightalt, KeyRightmeta and KeyNumlock, not guessed title casing.
- Wall-press fallback is NOT a normal crossing threshold. Zero disables only
  this fallback; the release keybind remains active. Preserve the full u32
  configured range, including values above the old GTK slider's 500 px cap.
  Apply buttons are explicit; draft changes do not flood the IPC queue.
- Wait for fresh ReleaseBind and ReleaseThreshold events before editing. Show
  canonical values while queued, wait for echo, and revert/report on three-second
  timeout. Queue failure retains the draft. Disconnect clears pending state,
  resets settings readiness and cancels the shortcut editor. Page change also
  cancels the shortcut draft. Timers wake once per pending save, not every frame.
- MyGo NumberInput has an 80-DIP input plus fixed-size stepper buttons. Setting
  the whole row to 100/110 DIPs makes the children overflow into the next button.
  Use 160 DIPs for these controls and a separate fallback-label row. Screenshot
  QA caught this; regression checks ensure the stepper does not overlap Apply.
  Tester clicks the labeled row's center, which can be BETWEEN input/buttons;
  test text editing by clicking inside the actual input area. Give modifier
  checkboxes distinct accessible labels so a single-key summary cannot swallow
  a test click intended for its identically named checkbox.
- A regression test proved that setting model.Error during a timeout build is
  too late for the banner already built earlier in that frame. Explicitly
  invalidate ONCE when expiring shortcut, threshold or layout saves. Without
  that wake the error stays invisible until another user event. Timer-advance
  tests now assert the banner appears without input and no polling remains.
- Go tests: 83 passed including subtests, no failures; evidence and renderer
  fixtures in target/mygo-qa/release-settings-20261028. Release-settings-540.png
  and Settings.png were visually inspected. These remain renderer fixtures,
  not native-window acceptance. Go vet, the paired Windows development build
  and darwin/arm64 frontend cross-compilation pass. Windows build log is in
  the same evidence directory; Mac compilation is not Mac runtime acceptance.
- Rust fmt/Clippy pass. Fresh workspace tests: 323 passed, 2 ignored, 1 filtered
  (the same environment-sensitive unroutable-address test). Logs are rust-tests.log
  and rust-clippy.log in target/mygo-qa/release-settings-20261028. Existing Cargo
  workspace default-features manifest warnings remain.
- Extended isolated real mDNS/DTLS smoke passes, including sided release keys,
  fallback 1500 -> 0 persistence and empty-bind default restoration. Evidence:
  target/mygo-qa/lan-smoke-20261028-001031-d5ed/result.json. Spawned PID checks
  precede mutations, dummy children are cleaned up; installed PIDs 17988/75732
  and production configuration remain untouched.
- Remaining migration gaps: full address/host/port/hook editing, RTT/network pin
  controls, About/version UI, Mac permission onboarding and physical/native
  acceptance. No commit, push, release, installation or startup registration.

### 2026-10-28: Compact UI supersedes exhaustive visible settings

- User explicitly wants a small, refined resident utility, not a full control
  panel. Screen layout is a polished convenience, not justification for a large
  window. Do NOT keep expanding the default UI for legacy feature parity.
- Default native content is now 680 x 420 DIPs, minimum 600 x 400. Removed the
  184-DIP sidebar, 28px page headings, header email, transport labels and per-peer
  Arrange buttons. A 50-DIP top navigation replaces them. Main cards use 14-DIP
  padding and 12-DIP gaps; requests and device actions fit the minimum size.
- Settings shows email and default-closed Advanced. Release, sharing, network,
  connection details, clipboard privacy and access removal are inside Advanced.
  Input capture/emulation recovery remains visible when unavailable. Tray/menu
  options and notification-area/menu-bar-only close policy are unchanged.
- Layout canvas is 240 DIPs high. Resolution labels are omitted, but dimensions
  still drive proportions and actual mapping. Drag/magnetic corner behavior is
  unchanged. After sidebar removal the canvas width is window width minus TWO
  14-DIP gutters; retaining the old width-minus-240 breaks pointer geometry.
- Hidden widgets do not build once MyGo Collapsible finishes closing. Pending
  shortcut, threshold, endpoint and network saves must sync outside the panel,
  even on another page; otherwise their deadlines disappear while hidden. New
  tests assert feedback without more input and no permanent polling.
- Finishes the connection editor WIP already started before this design
  correction, but keeps it nested under Advanced. Atomic UpdateConnection edits
  endpoint fields without replacing identity/sharing/layout. Endpoint changes
  reset transport before config mutation; hook-only changes preserve live state.
  Ignore stale DNS results for a different hostname. Network options have stable
  IDs despite RTT updates and retain offline pins. Native IPC revision is 3;
  previous revision-2 development binaries must be rebuilt together. Peer DTLS /
  KCP wire format remains unchanged.
- Go: 91 tests passed including subtests, vet passes. Compact bounds/default
  disclosure, all eight drag targets, hidden save deadlines and atomic endpoint /
  network controls are covered. Renderer fixtures in
  target/mygo-qa/compact-20261028 were visually inspected; these are NOT native
  screenshots. Latest Rust tests: 327 passed, 2 ignored, 1 filtered (the same
  environment-sensitive unroutable-address test); fmt and strict Clippy pass.
- Windows paired development build and darwin/arm64 frontend cross-compile pass;
  cross-compilation is NOT Mac runtime/package/backend acceptance. Isolated
  same-host mDNS/DTLS smoke passes, including atomic endpoint persistence with
  identity/layout/inactive state retained. Evidence:
  target/mygo-qa/lan-smoke-20261028-004712-582d/result.json. Installed PIDs
  17988/75732 were reverified at their installed path and remain untouched.
- README and docs/mygo-ui-progress.html describe this new design direction and
  show compact fixtures. Native Windows tray/IME/lifecycle acceptance is still
  pending after the earlier lock-screen stop; Mac permissions/menu bar/login,
  physical dual-machine input/return/key cleanup and idle CPU/memory remain
  unverified. No commit, push, release, installation or startup registration.

### 2026-10-28: Mac permission entry restored without expanding the default UI

- Previous goal turn made concrete compact-UI progress. This continuation
  rechecked the actual branch/worktree and tried an isolated Windows native
  fixture. Real UIA exposes MyGo controls, but image capture shows the Windows
  Spotlight/lock screen and activation fails. STOPPED GUI input and cleaned only
  path/argument-verified --no-daemon PID 78184. Current evidence:
  target/mygo-qa/compact-native-20261028-005501/result.json. Native tray/close/
  restart/IME acceptance is still blocked until the desktop is unlocked.
- Start-Process -WindowStyle Hidden can consume the first ShowWindow call via
  Windows STARTUPINFO. The isolated process initially had no visible window;
  a second --no-daemon launch handed over to the first and exposed its window.
  This is a hidden-helper launch artifact, not evidence that normal user launches
  start hidden. Do not use guessed window handles or bypass the locked desktop.
- Confirmed a real migration omission: backends only preflight TCC; deleted GTK
  code owned the prompt. MyGo now binds system ApplicationServices/CoreGraphics/
  CoreFoundation through purego, with no C compiler or restored GTK UI. The input
  card appears only for missing/unknown access or grant-related restart; normal
  healthy UI has no extra permission controls.
- Silent startup/activation checks plus Check again; no frontend polling timer.
  An explicit Open settings click requests one missing permission once per run,
  then opens its system pane. Never touch CoreGraphics permission APIs before AX
  is granted. Do not create event taps just to force a TCC listing. Request return
  is not approval, a frontend preflight is not daemon recovery, and grants require
  an explicit restart using the existing safe resident lifecycle. No OS grant
  implicitly authorizes a LAN device or enables clipboard.
- Framework pointer globals need dereferencing; exported CF callback structs need
  their addresses, NOT their first word. Match MyGo's static-native-global pointer
  binding pattern. A simple uintptr -> unsafe.Pointer conversion triggers Darwin
  vet; fixed and verified both darwin/arm64 and darwin/amd64 vet/compile.
- Clean owned-daemon exit is now surfaced as stopped input service, not silent
  indefinite Reconnecting. Still no automatic restart after TCC revocation, and
  attached independent daemons retain their own lifetime. Updated stale GTK-only
  permission comments and removed the unused AppleEvents usage-description key;
  native input uses CoreGraphics, not AppleScript automation.
- Go tests: 96 passed including subtests. Windows vet/build and both Mac frontend
  architecture builds, test compilation and vet pass. Rust fmt and macOS shell
  syntax checks pass. Evidence/renderer fixtures:
  target/mygo-qa/mac-permissions-20261028. Mac-input-access-600.png was visually
  inspected; it uses a mocked permission backend and is NOT native Mac evidence.
  No Mac APIs/TCC grants were executed or changed on this Windows host.
- Still need native packaged-app Mac checks: permissions must be attributed to
  Mousehop.app, not inherited from Terminal; actual grants, revocation, menu bar,
  login/restart, physical dual-machine input and idle cost remain unverified.
  Installed Windows PIDs 17988/75732 and settings are untouched. No commit, push,
  release, installation, startup registration or production-config mutation.

### 2026-10-28: Completion audit and layout-save lifecycle correction

- Previous goal turn was progress (Mac permission entry, clean-daemon-exit
  feedback and tests), not a completed goal. Current audit rechecks branch,
  manifests, deleted GTK folder, pinned upstream commits/licenses, interaction
  tests, resident policies and real isolated mDNS/DTLS behavior.
- Found and fixed layout's remaining hidden-state bug: acknowledgment/deadline
  logic lived only in layoutView and followed the selected peer. Switching page
  hid timeout handling; switching peers could leave a confirmed save pending
  indefinitely. syncLayout now runs before the banner on every page and tracks
  the originating peer. Deletion clears both gesture and save immediately in
  receiveEvent, before slab handle reuse can transfer them to a new computer.
- Added regression coverage for timeout after page change, acknowledgment after
  selecting another peer, and delete/recreate during both drag and pending save.
  No new UI controls or polling loop. Drag remains one atomic request on drop.
- Fresh Go run: 101 passed including subtests, vet passes. Paired Windows build
  and both Mac frontend architectures compile/vet. Fresh Rust workspace: 327
  passed, 2 ignored, 1 filtered (same unroutable-address environment-sensitive
  test); fmt and strict workspace/all-target/all-feature Clippy pass. Evidence:
  target/mygo-qa/layout-lifecycle-20261028. Fresh real mDNS/DTLS smoke passes:
  target/mygo-qa/lan-smoke-20261028-011803-f59c/result.json.
- Completion is NOT proven by these tests. The same external native-acceptance
  blocker persists across compact UI, Mac onboarding and this audit: Windows
  activation/capture was blocked by the lock screen in the last native probe;
  no user unlock confirmation or successful replacement native result exists.
  There is also no packaged Mac runtime acceptance for TCC/menu bar/login or
  physical Windows/Mac input/return. Adapter tests and cross-compiles are not
  substitutes. Next work requires an unlocked desktop and Mac native validation,
  not more main-page controls or speculative legacy-parity features.
- No GUI input was retried against the locked desktop this turn. All isolated
  dummy daemons are cleaned up by the smoke. No commit, push, release, install,
  autostart or production configuration changes. Preserve preexisting dirty
  diagnostics/HANDOFF work; do not stage the entire worktree indiscriminately.
- Goal is now marked BLOCKED, not complete, after the same native-acceptance
  gate persisted across three consecutive goal turns. Resume after the user
  unlocks Windows and provides/validates the Mac packaged-app runtime. Audit
  evidence and current source/binary hashes are in
  target/mygo-qa/layout-lifecycle-20261028/completion-audit.json.

### 2026-10-28: Device-local configuration and one email form

- User clarified that device-specific controls belong on Devices, not Settings >
  Advanced. Each saved/authorized device now has a Configure button; its panel
  expands immediately below that row. Only one device panel is open at a time.
  Sending behavior, connection details and device removal moved with outgoing
  peers; receiving clipboard, scrolling, sensitivity and access removal moved
  with incoming peers. Incoming entries remain reachable alongside outgoing
  clients, rather than being hidden whenever any outgoing client exists.
- Settings > Advanced now contains only application-level return shortcut,
  network and clipboard-privacy controls. Collapsing a device, switching devices
  or leaving Devices closes connection drafts, not pending saves. Confirmation
  and timeout handling remains global, with no new polling or protocol changes.
- Disclosure state is pruned after every model event, before deleted slab
  handles can be reused. Regression exposed a related snapshot bug:
  json.Unmarshal into an existing map retains keys missing from the new JSON
  object. AuthorizedUpdated now replaces the map, so removed access does not
  remain as a stale device/configuration entry. Never merge devices by name.
- User also flagged duplicate email configuration. The only email form is now
  in Settings. Devices has no email input; an unconfigured Nearby empty state
  offers Set discovery email, which navigates to that single form without
  writing configuration. Existing pairing/identity semantics are unchanged.
- Fresh verification: Go 113 passed including subtests, Windows vet and paired
  development build pass; darwin/arm64 and darwin/amd64 frontend build/vet pass;
  Rust fmt and strict all-target/all-feature workspace Clippy pass. Rust tests
  were not rerun for these frontend-only changes; prior baseline remains 327
  passed, 2 ignored, 1 filtered. Existing Cargo manifest warnings remain.
  Evidence/renderer fixtures: target/mygo-qa/device-config-20261028. The 600-DIP
  collapsed and expanded device fixtures were visually inspected; NOT native
  Configure interaction evidence. docs/mygo-ui-progress.html and READMEs updated.
- Resumed Windows native probe is no longer lock-screen blocked. Observed real
  compact pages, close removing the visible window while PID 83648 survived,
  and second launch reopening that same instance. Computer Use inventory does
  not expose the notification area; do not fabricate a tray window/handle.
  Later capture could show an occluding terminal despite the target UIA tree;
  activate the target and reobserve instead of acting on that mismatched image.
  Stopped automated GUI input when concurrent user input was detected. Native
  result: target/mygo-qa/resumed-native-20261028-070045/result.json.
- A second isolated native fixture was prepared after moving device settings,
  but the user redirected to email deduplication before native Configure QA.
  Both fixture frontends (83648, 84700) were path/argument checked and stopped;
  both dummy-daemon pairs report all exited. Installed PIDs 17988/75732 remain
  at their installed paths and were untouched. No startup registration, install,
  production config, commit, push or release. Latest device/email changes still
  need native interaction acceptance; Mac packaged TCC/menu-bar/login and
  physical cross-machine input/return remain unverified. Do not mark the full
  migration complete from these tests or compilation results.

### 2026-10-28: Interaction feedback and consistent application icons

- User clarified that UI work includes interaction, not only styling. Configure
  now uses MyGo's 200 ms height disclosure with a turning chevron, hover/press/
  focus states and Enter/Space support. It follows OS Reduce Motion and stops
  scheduling frames after settling. Closing panels disable their controls;
  switching devices closes unsent drafts, not submitted confirmations.
- The one Settings email form disables unchanged/pending saves, accepts Enter,
  shows Saving until canonical echo, then Saved briefly. Queue failure, daemon
  rejection and a three-second command-echo timeout retain the draft for retry.
  Clean editors follow external updates; unsent drafts are not overwritten.
  Nearby Connect suppresses duplicate submits until a canonical saved-client
  event, not Alive. Three-second frontend acknowledgment is separate from the
  backend's approval retry. Escape cancels an unsent connection draft. Pending
  feedback runs globally across pages with scheduled deadlines, not idle loops.
- Pitfall: actual backend connection errors are in transport_status (Failed:),
  not just a hypothetical last_error field. Saved rows now show short sanitized
  failure status and never raw certificate text. Added coverage for that actual
  serialized state, progressive disclosure, reduced motion, feedback, retries,
  hidden-page deadlines and no frames pending after animations settle.
- Found three real icon omissions: frontend EXE had no icon resource, window
  never called SetIcon, resident icon was an unrelated temporary two-screen
  drawing. Reuse the canonical existing ICO for application/Windows icons and
  canonical tray SVG for a black-alpha Mac template. Embedded PNGs are generated
  by scripts/generate-mygo-icons.go; go generate is reproducible. No new brand
  artwork or image generation. Mac uses IconIsTemplate, still requires native
  light/dark appearance acceptance; app bundle ICNS remains in packaging.
- Windows build uses pinned build-only github.com/akavel/rsrc v0.10.2 to embed
  all seven ICO sizes. Refuse to overwrite a preexisting arch-specific syso and
  remove the owned temporary file in finally. scripts/verify-mygo-icons.py parses
  actual PE resource trees and hashes every image against the canonical ICO;
  CI checks both the launcher and frontend. No temporary syso remains.
- Final fresh checks: 123 Go tests passed including subtests; Windows vet,
  paired build, both EXE icon-resource verifiers pass. Both darwin/arm64 and
  darwin/amd64 frontend build, test compilation and vet pass. Rust fmt and
  strict workspace/all-target/all-feature Clippy pass. Rust source unchanged
  this turn: prior test baseline 327 passed / 2 ignored / 1 filtered was NOT
  rerun. YAML syntax (not live Actions execution) and PowerShell parser pass.
  Existing Cargo default-features warnings remain. Evidence:
  target/mygo-qa/ui-icons-20261028-073852/result.json. HTML progress report and
  READMEs updated; renderer device fixtures and Mac template preview inspected.
- Native probe again encountered Windows Spotlight lock screen; activation
  timed out despite a returned MyGo UIA tree. STOPPED GUI input and asked the
  user to unlock; do not retry until confirmed. Frontend PID 84936 was verified
  by development path/--no-daemon and stopped. Own dummy PIDs 71148/82428 exited,
  recorded in daemons-cleaned.json. Installed PIDs 17988/75732 reverified at
  their installed paths, untouched. frontend-session.json records an earlier
  probe build hash; final rebuilt binary hash is in result.json and icon checks.
- Latest interaction/native icons, tray menus/restart, Chinese IME, packaged
  Mac TCC/menu bar/login, physical Win/Mac input/return/key cleanup and idle
  CPU/memory remain unverified. Cross-compiles, resources and renderer tests
  are not native acceptance. No commit, push, release, install, autostart change
  or installed-config mutation. Do not call the migration fully accepted.

### 2026-10-28: Unlocked Windows native acceptance and mDNS duplicate fix

- User asked to continue. Fresh Computer Use inventory and actual fixture
  screenshot confirmed an unlocked desktop. The first capture showed an
  occluding terminal despite the target UIA tree; activation and immediate
  recapture showed the app. Do not treat an occluding image as the app, and
  never automate the terminal to fix focus.
- Native Windows observations: original purple brand icon in the title bar;
  Configure expands directly below its row; Space collapses it with a visible
  focus ring; Settings has one email input, unchanged Save disabled, Advanced
  closed by default and global-only after expansion. Saving a synthetic email
  showed Saved after a real isolated-daemon echo. No privacy/access toggles or
  startup settings were changed through GUI automation.
- Dragged the remote screen from right to left with the mouse while the local
  screen stayed fixed. Isolated config persisted position=left and
  alignment=top_left. Close removed the native window after event processing,
  PID 7696 stayed alive, and a second launch reopened that same window/PID with
  the left layout retained. The immediate list_windows after Close can race the
  hide event: reobserve before declaring a lifecycle failure.
- Found a real native defect: mDNS changed an already paired peer hostname
  from ...native-b.local to ...native-b-2.local, causing an extra Connect row.
  Same fingerprint/address/port in captured events established it was the same
  fixture endpoint, not an unrelated device with a similar name. Presentation
  matching now checks service port plus normalized host or known numeric IPs
  (fixed/resolved/active), including normalized IPv6. Never merge by display
  name. This ONLY suppresses duplicate UI rows and resolves pending UI feedback;
  it does not authorize, update pins, trust discovery metadata, modify backend
  ConnectDiscovered lookup, or change IPC/peer protocols.
- Added native-case regression plus host case/final dot, fixed/resolved IP,
  IPv6 active endpoint, different-port, same-name/different-endpoint and invalid
  address cases. Pitfall: MyGo Tester.HasText is substring matching; Connect
  also matches Connected. Use exact slices.Contains(Texts(), "Connect") when
  asserting that action is absent/present, including the rejection retry test.
- Final fresh Go: 132 passed including subtests, Windows vet/build and frontend
  seven-size PE icon verification pass. Final frontend SHA-256:
  617fe1405aefe02092bb85bc762715396fd62b10b37e99e91e20825f479c01a3.
  Both Mac frontend architectures compile/vet, not native runtime. Rust was not
  changed this turn; prior fmt/Clippy/test results are explicitly historical.
- Final-build native recheck reproduced BOTH mDNS hostname variants, displayed
  no duplicate Connect and reconfirmed Configure/Space/title icon. Evidence:
  target/mygo-qa/native-final-20261028-075757/result.json and its recheck-path.txt
  pointing to native-recheck-20261028-080746. First probe's UIA observations and
  saved config refer to the pre-fix build; final recheck records the final hash.
- Hidden-window frontend-only 5.009-second sample: 0.015625 CPU seconds (~0.31%
  of one core), ~80.3 MiB working set / ~120.9 MiB private bytes. This is a short
  dummy-fixture diagnostic, NOT production idle or total-app acceptance.
- Path/--no-daemon verified and stopped owned frontends 7696 and 13060. Both
  dummy pairs exited via their own cleanup: 82560/37088 and 78676/78856.
  Installed 17988/75732 reverified at installed paths and untouched. No commit,
  push, release, install, startup registration or installed-config changes.
  Notification area is not exposed by the supported Computer Use inventory:
  tray clicks/menu/Restart remain unverified; do not fabricate shell handles.
  Mac packaged permissions/menu bar/login, Chinese IME and physical Win/Mac
  crossing/return/key cleanup still require acceptance. No full-completion claim.

### 2026-10-28: Native cancellation, incoming wake and resident diagnostics

- Continued on the existing final frontend (SHA-256 617fe1405aefe02092bb85bc762715396fd62b10b37e99e91e20825f479c01a3).
  Escape closed an unsent connection draft; releasing a dragged screen outside
  the canvas cleared its preview. The isolated config retained its original
  hostname and right/proportional layout. Neither cancellation persisted a change.
- After hiding the page, a new isolated incoming request restored the same
  resident window without launching the frontend again. Settings stayed selected
  and the request appeared above its content, with name/address and Allow/Decline
  but no raw fingerprint in the accessibility tree. No permission button was
  clicked; the fixture still reported an empty authorized map.
- Border and system-Size attempts did not change captured dimensions. This is
  NOT native minimum-window-size acceptance; keep renderer tests separate.
- Six 10-second samples: static visible Settings frontend averaged 8.360% of
  one core; hidden frontend averaged 0.207%. Visible working set was about
  81.38-82.03 MiB, private memory 119.85-120.25 MiB. These are debug/dummy-fixture
  observations, not production or physical-backend benchmarks. Local application
  footprint includes its Rust daemon; the second daemon is peer-test overhead.
- Do not assume a repaint/IPC loop from that CPU reading. Builtin
  MYGO_FRAME_STATS=all showed zero new frames during a separate 10-second
  7.962%-of-one-core sample; a read-only IPC check found no continuous State
  stream. A temporary ignored source copy added pprof, not production code.
  Its profile mostly attributed samples to runtime.cgocall/GetMessageW, including
  a blocking message wait; these percentages are NOT attribution of actual CPU
  work. Native/driver attribution remains unresolved. Forcing MYGO_GPU=0 in an
  isolated launch still measured 7.496% of one core: do not disable GPU by default
  or speculate that application animations are the cause.
- Sampling pitfall: Start-Process -WindowStyle Hidden can suppress the initial
  window/frame. That fresh process's zero-CPU sample was hidden, not visible.
  A second launch restored it, but the first screenshot showed an occluding
  terminal despite the Mousehop accessibility tree. The resulting 0.156% sample
  is an occluded baseline, NOT proof of screenshot-tool overhead. Activate and
  reobserve the actual app before claiming foreground acceptance. Never automate
  the occluding terminal or fabricate notification-area targets.
- After activating and visually confirming the same process's foreground
  Devices page, a further 30-second sample measured 9.217% of one core. The
  visible CPU concern remains unresolved; zero new frames in the separate
  10-second sample does not establish the cause of native thread work.
- Owned diagnostic frontends 83760, 81640 and 84280 were path/argument checked
  and stopped; dummy PIDs 80336/78768 exited through their fixture cleanup.
  The primary boundary fixture's earlier pair also reports all exited.
  Previously recorded installed PIDs 17988/75732 were absent at the final
  query, so their paths could not be reverified this time; no installed binary,
  startup registration or configuration was modified. Final deliverable hashes
  remain unchanged. No commit, push, installation or release.
- Evidence: target/mygo-qa/native-boundaries-20261028-082709/result.json and
  target/mygo-qa/native-frame-probe-20261028-083806. No production source or final
  binary was changed for these checks; 132 Go tests and platform builds are the
  previous verified baseline, not freshly rerun. Notification-area menu/restart,
  native minimum resize, Chinese IME, packaged Mac permissions/menu bar/login and
  physical Win/Mac crossing/return/key cleanup remain unverified.

### 2026-10-28: CPU root-cause probe awaiting desktop unlock

- User explicitly requested finding and fixing the CPU cause, not accepting
  the current foreground cost. New isolated fixture:
  target/mygo-qa/cpu-rootcause-20261028-090854. Computer Use activation timed
  out; a refreshed capture showed Windows Spotlight lock screen despite a
  Mousehop accessibility tree. Stopped GUI input and requested unlock. Do not
  retry GUI interaction until the user confirms unlocking.
- Prepared an ignored .NET 8 diagnostic sampler at
  trace-reader/bin/Release/net8.0/trace-reader.exe within that fixture. It only
  accepts workspace target processes. It measures per-thread CPU, queries native
  thread start addresses, captures short-suspended thread contexts/stacks with
  DbgHelp and resumes threads in finally; symbols resolve AFTER suspension ends.
  This is process diagnostics, not a GUI automation substitute. Use supported
  Computer Use for all app/window input. Call from repo root with PID and sample
  seconds; redirect output to a file (two JSON objects) rather than flooding the
  context. System public symbols resolved successfully without administrator.
- A locked-desktop 10-second sample consumed zero CPU and showed GetMessageW /
  WaitForSingleObject blocked stacks. This is NOT a foreground reproduction and
  does not prove the CPU cause. No production patch or deliverable rebuild yet.
  WPR is available but the shell is not administrator; no ETW session was started.
- After unlock: relaunch the isolated dummy pair and final frontend; visually
  verify actual foreground app (not an occluding terminal); sample hot native
  threads, compare their CPU deltas and native call paths, then fix the identified
  cause and repeat the same foreground/hidden measurement. Prior zero-frame and
  software-renderer evidence still prohibit assuming a repaint loop or disabling
  GPU as an unproven workaround.
- Path/--no-daemon checked and stopped own frontend 68516; dummy PIDs
  48808/56600 report all exited. No installed configuration or startup setting was
  touched. Probe state and cleanup are in the fixture result.json.

### 2026-10-28: Foreground CPU inflated by Computer Use accessibility observation

- User explicitly clarified ALL frontend UI and interactions must remain MyGo
  Native. The entry point is mousehop-mygo/main.go, Content: ui.View(a.view).
  No WebView/HTML replacement, GPU disabling or accessibility removal was made.
- Native stacks in target/mygo-qa/cpu-fix-20261028-092139 attribute the foreground
  work to the main thread and a UIAutomationCore worker, not the waiting NVIDIA
  worker. Instrumented ignored toolkit copies saw about 50,000 QueryInterface
  calls per five seconds and repeated full property/navigation traversals even
  with no new frames. Stack samples alone did NOT identify the caller.
- A short user-mode ETW session was available without administrator privileges:
  Microsoft-Windows-UIAutomationCore, GUID 820A42D8-38C4-465D-B64E-D7D56EA1D612.
  Event 509 explicitly maps client PID 6996 (codex-computer-use.exe) to frontend
  PID 38392, including UiaNode_GetPatternProvider, BulkFetch and property calls.
  Evidence: uia-client-targets.json and uia-client-target-evidence.xml. Do not
  attribute these observed queries to unrelated desktop/security applications.
- Same unchanged frontend PID/window, Screen layout page: observer attached
  20.005 s / 1.796875 CPU s (8.981% of one core); resetting the supported Node
  kernel removed both Computer Use helper processes. A subsequent 30.029 s
  sample consumed 0.171875 CPU s (0.572%). Reimporting Sky restored observation
  and a 20.006 s sample rose to 2.03125 CPU s (10.152%). Screenshots before and
  after showed the same visible page. The old dummy pair had expired, so this
  A/B/A identifies observer overhead but is not connected-device acceptance.
- WM_GETOBJECT strict-root-only handling did NOT lower CPU (11.331% of one
  core in its diagnostic sample). It stays in an ignored copy, NOT production.
  RpcServerInqCallAttributes returned RPC_S_NO_CALL_ACTIVE; process-handle scans
  did not identify a caller. Avoid repeating these failed attribution attempts.
- Measurement pitfall: include_text:false does NOT turn off the helper's
  background accessibility observation. Leaving Sky idle is not a clean CPU
  control. Use the documented Node kernel reset after recording actual visible
  state, measure independently with no helper processes, then reinitialize Sky
  for an after-sample screenshot. Do not compare foreground against hidden or
  occluded windows, kill tool helpers directly, or disable app accessibility.
- ETW all-keyword capture is very verbose (328 MB in the first probe; the XML
  conversion was 2.7 GB). Keep captures short and analyze compact client/target
  summaries. Native stack sampling remains useful for actual thread CPU; Go
  pprof's GetMessageW wall-time attribution is not proof of busy computation.
- Fresh connected control: target/mygo-qa/cpu-observer-control-20261028-095627.
  Same final frontend PID 80004, visible Devices page, real same-host DTLS with
  dummy capture/emulation. With observation: 20.019 s / 1.734375 CPU s (8.663%).
  After supported kernel reset: six consecutive 10-second samples, 60.044 s /
  0.03125 CPU s (0.052%). No helper processes at every sample boundary or end;
  before/after screenshots show the same visible Connected page. Seven new frame
  log entries during the clean sample do not prevent near-zero idle UI CPU.
- Same clean 60-second sample: local Rust daemon 2.967% of one core, remote
  dummy-fixture daemon 3.253%; the remote one is not local application cost.
  Frontend max working set 66.53 MiB, private bytes 110.53 MiB. These short debug,
  dummy-backend measurements are NOT a physical Win/Mac or production benchmark.
- Fresh Go test -count=1: 132 tests including subtests passed; Windows vet passed.
  No production source/dependency/deliverable changes. Final frontend SHA-256
  remains 617fe1405aefe02092bb85bc762715396fd62b10b37e99e91e20825f479c01a3;
  backend remains 8be801f04065acc37db211436ed1540ecde319a15f09f7bb490f5d0c170da554.
  Mac builds and Rust suite were not rerun this turn. Do not call this an app
  source-code CPU fix: the identified overhead was the observation session.
- Owned frontends 38392, 57352, 81932 and 80004 were executable-path and
  --no-daemon checked before stopping. First dummy pair 58212/80056 expired
  through its cleanup; fresh pair 80804/32948 exited through stop-daemons.
  result.json records the connected control, test exits and final cleanup.
  No installed app/config, autostart, TCC, commit, push or release was changed.

### 2026-10-28: MyGo release-profile packages for physical testing

- User requested Windows/Mac test packages, explicitly using ssh mac and
  delivering the Mac DMG into Downloads. Built Windows x64 locally and macOS
  ARM64 natively on the connected macOS 27.0.1 host, with overlapping work.
  This is the dirty mygo worktree at base 86c0c2f4c6016c1a5055f56681a89f73b984567c,
  not a clean tagged release. Version stays 0.17.20; filenames identify MyGo.
- Delivery: dist/mygo-test-0.17.20-20261028-101013/. Windows installer:
  Mousehop-0.17.20-mygo-20261028-101013-windows-x64-Setup.exe (11,606,449 bytes),
  SHA-256 960a66a0a9e3d0dd866796b83030c7097f40b70a13af1822e130004a97830f97.
  Portable ZIP: Mousehop-0.17.20-mygo-20261028-101013-windows-x64-portable.zip
  (11,578,976 bytes), SHA-256
  7c82d6f5bfd6a599816812006fa6879a2430c9f333fd69df6fdcbbfae1506645.
- Final Mac DMG: Mousehop-0.17.20-mygo-20261028-101013-macOS-arm64.dmg
  (12,660,067 bytes), SHA-256
  bf90fcd8e044cdd18a582a57308d28fe51cde69d846ea093aaf11eaf9d660c53.
  Same verified bytes are in /Users/alvin/Downloads/ and the Windows delivery
  directory. Mac source/build is in
  /Users/alvin/.cache/mousehop/mygo-package-20261028-101013. No Intel build.
- Both platforms use the same 261-file source snapshot; all Windows workspace
  hashes were reverified after packaging and Mac archive hashes before build.
  Only two existing shell scripts were LF-normalized in the archive. Source
  tar, SOURCE-MANIFEST.json, BUILD-PROVENANCE.json, SHA256SUMS.txt and TESTING.md
  accompany the packages. Evidence: target/mygo-package-20261028-101013/.
- Windows Rust: optimized release profile, fat LTO, panic=abort, static CRT;
  both EXEs are x64 GUI-subsystem PE files with system-only static imports,
  no GTK/Adwaita or external VC runtime DLLs. All seven icon sizes match the
  canonical ICO in BOTH EXEs. ZIP CRC/payload hashes and Inno compilation pass.
  Installer is unsigned. Windows Go tests: 132 passed including subtests, vet
  passed; packaged Release daemon passed isolated mDNS/DTLS approval, certificate
  pinning and atomic placement/connection-update smoke tests.
- Mac: Rust optimized release and native Go 1.27.3, ARM64 in both binaries;
  bundle main executable mousehop-mygo, com.mousehop.Mousehop, LSUIElement=true,
  and exact version 0.17.20. Mach-O-derived minimum OS is 13.0; actual runtime
  version smoke was on macOS 27.0.1 only. System-only dependencies, ad-hoc signing,
  deep strict signature verification, DMG integrity/read-only mount and payload
  hashes pass. Not notarized. Mac Go tests: 124 passed including subtests, vet
  passed; counts differ because platform-specific tests are selected by Go.
- Packaging repair: shell sed version extraction previously left CR from a
  Windows CRLF Cargo.toml, leaking a newline into CFBundleVersion. Consume the
  rest of the line in build-mygo.sh/package-macos.sh and validate numeric bundle
  versions. Both parsers pass actual LF and CRLF fixtures; final bundle fields
  are asserted exactly, not just substring-matched CLI output. The initial bad
  DMG was moved out of Downloads to isolated rejected artifacts and replaced
  only with the final hash above. Do not deliver its ec0432fe... hash.
- Test/tool pitfalls: long macOS default TMPDIR made supervisor AF_UNIX test
  sockets exceed the path limit. Rerun with isolated /tmp/mhgo-101013; all 124
  tests passed without a product-code workaround. Go toolchain module ZIP names
  include golang.org/toolchain@version/ (two path components); the usable cached
  SDK is toolchains/go1.27.3-darwin-arm64/toolchain@v0.0.1-go1.27.3.darwin-arm64/.
  Do not redownload simply because the first assumed bin/go path is absent.
- No installed app replacement/restart, autostart, installed config, TCC, commit,
  push, tag or publication. No full Rust suite or physical cross-screen/tray/menu
  acceptance this turn; user will perform the physical tests. Quit old resident
  apps before testing to avoid attaching the new UI to an incompatible daemon.

### 2026-10-28: Nearby updates, live access revocation and local device name

- User reported Windows-first/Mac-later email discovery lag, then removing Mac
  "Can control this computer" access without disconnect/Windows refresh. Also
  requested the page's Mousehop header become the local device name.
- Preserved read-only Sync snapshots, hashes and warning logs before testing:
  target/nearby-revoke-20261028/. Do not treat missing INFO-level discovery
  messages in durable warning logs as evidence that discovery never happened.
- CONFIRMED security/lifecycle defect: remove_authorized_key changed the shared
  map, but existing authenticated sessions were never closed. Reproduced with
  isolated Windows daemons AND Windows-to-Mac dummy daemons over the real LAN.
  Baseline stayed alive through the 3-second budget and another 4 seconds.
- Listener now retires the exact session/slot before bounded close; queued
  input/accept events cannot use that token. Emulation checks current shared
  authorization on accept/messages and disconnects revoked peers on settings
  updates. Existing disconnect cleanup forgets emulation input state. Service
  rejects already-queued Connected/Entered events for revoked identities.
- A disconnect before first screen entry previously had no incoming barrier,
  so service omitted IncomingDisconnected. Always publish terminal UI state for
  the current generation. Go AuthorizedUpdated also prunes only revoked incoming
  live/locked state. Do not confuse reconnect attempts with successful control:
  an enabled outgoing device can retry/request fresh approval after revocation,
  but must not become alive without consent. No new wire-level reason was added.
- Discovery risk correction: mdns-sd 0.19.1 documents in-place register updates.
  Do not unregister the same fullname before updating TXT/IP: unregister queues
  a delayed goodbye that can overlap the new records. Only unregister a genuinely
  different fullname. No high-frequency polling/timer was added.
- Evidence boundary: original long asymmetric delay NOT reproduced. Cross-host
  baseline discovery was 1.838s, fixed one run 0.955s. This is not statistical
  latency acceptance or proof of the exact intermittent incident's cause.
- LocalDeviceName IPC event uses the same local_device_name helper as mDNS;
  toolbar AND local layout tile show it, with single-line clipping and full-name
  hover. Tests verify normal/long names at both widths and hover interaction.
  Application/window/tray branding remains Mousehop. IPC revision is now 4;
  update both binaries and test fixture version checks, reject old IPC 3 daemons.
- Verification: affected Rust crates 197 passed/1 ignored; Go 134 Windows and
  126 macOS passed including subtests; Go vet on both platforms; Rust fmt and
  workspace all-target/all-feature clippy pass (existing Cargo feature warnings).
  Windows debug build includes original icon resource; macOS ARM64 Release
  backend and native Go frontend build. Full LAN smoke and all 9 final cross-host
  presence/revocation/name checks pass, using PID-verified isolated dummy fixtures.
- Mac shell cargo symlink targets missing /opt/homebrew/bin/rustup-init. Use
  /Users/alvin/.rustup/toolchains/1.95.0-aarch64-apple-darwin/bin in PATH. Go tests
  still require short TMPDIR; used /tmp/mhgo-presence-20261028. Cross-host helper
  must save --mac-binary before stripping its custom args, otherwise it tests
  the installed old binary and correctly fails the IPC revision guard.
- Report: docs/diagnostics/2026-10-28-mygo-presence-and-device-name.html. Renderer
  PNGs inspected at target/nearby-revoke-20261028/screens/. Not installed UI QA
  or real held-key/physical cross-screen acceptance. Input auth check is an
  in-memory map read, not an OS/network query; no production CPU benchmark run.
- No installed app/config, autostart, TCC, commit/push or publication changed.
  The Mac owned build cache mygo-package-20261028-101013 now contains IPC4 patched
  sources/builds, NOT the original source snapshot. Original delivered installers,
  DMG, source archive and hashes remain unchanged IPC3 and lack these fixes.
  No replacement installers/DMG were packaged in this turn.

### 2026-10-28: Screen-layout thumbnails normalize the shared edge

- User wants screen-layout devices to look equal-sized rather than making Mac
  smaller because its resolution is lower. Implemented visual normalization:
  left/right neighbors have equal heights, top/bottom have equal widths; the
  other edge scales proportionally. Same-aspect screens become identical-sized
  rectangles regardless of independent resolution/density scaling.
- Changes are in mousehop-mygo/layout.go and layout_test.go, plus docs. Backend
  dimensions, overlap restrictions and crossing coordinate transforms are NOT
  changed. No IPC/wire change, no additional option or timer. Unknown or zero
  geometry uses the known peer's aspect ratio, or the neutral fallback.
- Reserve space for both normalized orientations so the local rect stays fixed
  during side changes. placedScreen normalizes before placement; snapping uses
  rectangle centers rather than top-left corners because the guest can resize.
- Pitfall: equal shared edges collapse both corner outlines onto one rect.
  Preserve access to all eight anchors: a small signed cross-axis drag chooses
  top/bottom or left/right; a centered pointer retains the prior selected anchor.
  Test drags must express that intent with signed offsets, not identical pointer
  locations for indistinguishable corner outlines. No new corner buttons.
- Windows Go suite 141 passed, macOS native suite 133 passed, including subtests;
  vet passes on both. Windows icon-bearing debug frontend/backend build and Mac
  native frontend build pass. Existing drag/cancel/timeout/single-save tests pass.
  Additional tests cover resolution-independent size, aspect preservation,
  Win/Mac and portrait inputs, missing/zero geometry, all anchors and canvas fit.
- One initial Windows run failed the unrelated supervisor helper PID-ready-marker
  assertion. Full rerun passed and isolated supervisor test passed three repeats
  without source changes. os.WriteFile/readiness polling could race, but the
  exact transient cause was not captured; do not call it a product layout defect.
- Inspected renderer images for equal-height and equal-width layouts under
  target/layout-equal-edge-20261028/screens/. Report:
  docs/diagnostics/2026-10-28-screen-layout-equal-edge.html. These are fixture
  renderings, not installed-app visual or physical-input acceptance.
- No installed app/config, previous installer/DMG, autostart, TCC, commit/push or
  publication changed; no new packages. Mac owned build cache now also includes
  these layout changes. Rendering adds bounded arithmetic only, no polling loop.

### 2026-10-28: approved dual-endpoint installation; Mac accessibility blocked

- User explicitly approved Windows and Mac installation and interruption. Installed
  `dist/mygo-test-0.17.20-20261028-122550` without source edits, publication,
  startup registration changes, config/certificate deletion, or TCC changes.
- Windows Setup SHA-256 verified as
  `204b14be254a0ac59ec22346be7437c74f736a1f2471b9f1ec63a831fc664a4e`.
  Existing HKCU Inno installation is
  `C:\Users\zye.WD\AppData\Local\Programs\Mousehop`; the separate
  Mousehop-KCP-Test install was untouched. Full install/config backups:
  `D:\git_projects\mousehop\target\diagnostics\install-20261028-1330\Mousehop-install.backup`
  and sibling `Mousehop-config.backup`. Exact-path checked old app/daemon and
  development preview only were stopped. Per-user Inno installer exited 0 using
  existing directory, /NORESTART and /NOCLOSEAPPLICATIONS; setup log is sibling
  `windows-setup.log`. No UAC bypass. New launcher/frontend/daemon PIDs:
  64696/40856/77956, all at installed bin paths; frontend Responding=True.
- Windows installed payload equals staged package: mousehop.exe SHA-256
  `a274f48fbd193dfb0a9f527d8d4eea3b732a7cfbd20fcf96fc0dc7b20e9705f7`,
  mousehop-mygo.exe
  `88971ebc4ce9cb7abb2a6128a4fc8de134c2ff8dbb86b593afca74f40f98a03c`.
  Read-only Sync responds; CaptureStatus/EmulationStatus Enabled. Config and
  certificate SHA-256 unchanged. Peer snapshot is Disconnected, alive=false.
  Recent logs include restart-time DTLS disconnect and repeated Windows
  `failed to translate to windows scancode` warnings; not repaired here.
- Mac SSH cannot list Downloads: `Operation not permitted`. Do not alter TCC to
  bypass this. Copied the identical local DMG into accessible owned directory
  `/Users/alvin/.cache/mousehop/install-20261028-1330/package.dmg`; SHA-256
  `e742f7cbb2314872b3cb97addd9e9a9c130c62524efd1a4c3ee79caec0e6b274`
  verified, hdiutil verification passed, read-only mounted payload and strict
  deep codesign verification passed. Mount detached after installation.
- Inspected actual `/Applications/Mousehop.app` and processes before changes.
  Backups in `/Users/alvin/.cache/mousehop/install-20261028-1330/`:
  `Mousehop.app.backup` (ditto copy), `Mousehop.app.original` (moved original),
  and `config.backup`. Wait for SIGINT daemon/SIGTERM frontend exit before
  replacing: immediate kill -0 still saw exiting processes, so the first attempt
  stopped before replacement; subsequent check confirmed both exited.
- Mac app opened through LaunchServices (`open /Applications/Mousehop.app`),
  not shell daemon. New frontend/child daemon PIDs 46663/46674 at installed
  Contents/MacOS paths. Installed executable hashes equal mounted DMG payload:
  mousehop `269d3780042f36e7908bdb7614f81cc5b8d15062e1e36b039640a1f4abff1b06`,
  frontend `7070c7354ec317a4aeb53a5bd5816e50424f321a23ee7fec8f4612c8ec421551`.
  Config and certificate hashes unchanged; old diagnostic collector untouched.
- BLOCKER: Mac Sync responds but CaptureStatus=Disabled; EmulationStatus=Enabled
  alone is NOT permission success. Log at 2026-10-28T05:29:47Z explicitly reports
  accessibility permission required, capture no backend available, and emulation
  accessibility warning. Reported immediately; no TCC manipulation or fabricated
  approval. User must inspect macOS Privacy & Security > Accessibility and grant
  intended installed Mousehop app permission through normal UI, then arrange
  a permitted restart/recheck. No physical cross-screen acceptance performed.
  Logs remain `/Users/alvin/Library/Logs/Mousehop/connection-daemon.log` and
  Windows `%LOCALAPPDATA%\Mousehop\logs\connection-daemon.log`.

### 2026-10-28: MyGo localized UI packages delivered

- Built from the dirty `mygo` worktree at base `86c0c2f4c6016c1a5055f56681a89f73b984567c`, using one 276-file source snapshot verified against the Windows and native Mac build inputs. Unique evidence is `target/mygo-package-20261028-122550/`; delivery is `dist/mygo-test-0.17.20-20261028-122550/`.
- Windows x64 installer and portable ZIP were built and audited: static Rust CRT, PE GUI subsystem/system-only imports, canonical seven-size icons in both EXEs, Inno compile, ZIP CRC/payload checks. Installer is unsigned.
- Mac ARM64 DMG was built natively on macOS 27.0.1, with cached Go 1.27.3 and isolated short `TMPDIR`; bundle version/identifier/executable/LSUIElement, arm64, system-only dependencies, ad-hoc signature, strict deep signature verification, DMG verify and read-only mount payload checks passed. DMG was copied to `/Users/alvin/Downloads/`; no installed app was replaced or restarted.
- Go tests including subtests: Windows 161 pass events, Mac 153 pass events; Go vet passed on both. Full Rust suite, physical cross-screen/native tray acceptance, and broader macOS runtime compatibility were not run. Not notarized or published. SHA-256 and provenance are in the delivery directory.

### 2026-10-28: focus clipboard sync production wiring

- The service now owns an optional focus manager on Windows/macOS. The native
  backend is created once at service startup and retains one shared suppression
  list plus one `ClipboardEmulation`; writes must recheck the native generation
  before and after the synchronous OS write. Other platforms keep the manager
  absent, and the old orchestrator/native prototype files are removed.
- Each outbound and inbound lane is keyed by direction plus its exact session
  counter. Service snapshots filter by the bilateral per-device grant, detach
  stale/replaced lanes, attach new lanes with text+PNG consent, and only runs
  the manager tick when a lane exists. Focus triggers resolve the configured
  outbound fingerprint or authenticated incoming fingerprint; a missing
  fingerprint is logged and ignored.
- Do not treat a post-write native sequence number as ownership proof. The
  adapter's generation fence prevents obvious stale writes, but external
  clipboard applications can race it, native APIs can block, and arboard/native
  permissions remain OS boundaries. The suppression list is updated in place so
  policy changes do not accidentally replace the backend or its worker state.
- Final validation passed on Windows: formatting check, locked all-feature
  workspace tests (448 passed, 3 ignored), strict all-target/all-feature
  Clippy, and `git diff --check`. Native ARM64 Mac validation also passed:
  workspace all-target Clippy and serial tests for `mousehop` (253 passed, 1
  ignored), `mousehop-input-capture` (71), and `mousehop-input-emulation` (90).
  The directory names `input-capture` and `input-emulation` are not Cargo
  package IDs; use the `mousehop-` names above for targeted Mac tests.
- Validation still needs to distinguish Rust tests/clippy from physical
  Windows/macOS clipboard acceptance. No installer, TCC setting, deployment,
  commit, or cross-device native clipboard test is implied by source validation.

## 2026-10-29: native clipboard write race fencing

- Completed `target/codex-tasks/focus-native-write-race.md`. Generation-fenced Windows writes now prepare UTF-16/PNG data before opening the clipboard, retry `OpenClipboard` 10 times at 10 ms intervals, compare the expected sequence while holding the native lock, publish in one `OpenClipboard`/`CloseClipboard` transaction, and read the resulting sequence before releasing the lock. A stale generation returns without clearing the clipboard.
- macOS generation-fenced writes prepare and validate UTF-8/PNG data before the first `changeCount` read, use the `clearContents()` return value as the write generation, and reject a changed `changeCount` after `setData`. The code documents that NSPasteboard has no Windows-style lock, so the post-write check is the available race detector.
- Added preparation/size-limit tests and the ignored real native text+PNG generation test. The Windows ignored test was run but could not open the current system clipboard: `OpenClipboard` returned `ERROR_ACCESS_DENIED` (`os error 5`) after the bounded retries. PowerShell `Set-Clipboard` was available, but an independent Win32 open also returned access denied; no permission or system-lock bypass was attempted.
- Windows verification passed: `cargo fmt --all -- --check`; `cargo test --workspace --all-features --locked --target-dir target/codex-race` (450 passed, 4 ignored); strict all-target/all-feature Clippy (0 errors; existing Cargo warnings); and `git diff --check`.
- Mac ARM64 verification passed in `/tmp/mousehop-race-20261028-235325-clean` with the native Rust 1.95 toolchain: workspace Clippy `-D warnings` (existing Cargo/IOHID/ar warnings only), serial `mousehop` tests (253 passed, 2 ignored), and the ignored native text+PNG test (1 passed). The ignored test changed the Mac system clipboard during its run. No commit, install, deployment, or TCC change was performed.


## 2026-10-29: clipboard release install and known limitations

- Installed fresh snapshot `clipboard-release-20261029-001307-511877` as version 1.0.0 on Mac ARM64 and Windows x64; exact package and installed binary hashes, backups and logs are in `target/release-pipeline/README.md`. Both frontend and daemon processes were observed, installed executables matched their build stages, and no panic log was present. This is not native clipboard or dual-device acceptance.
- Mac Accessibility `ax-probe` returned 1; the installed daemon logged permission denied and capture backend unavailable. User must manually re-authorize the installed app in System Settings; never change TCC programmatically. Windows was locked during the earlier real clipboard write probe, which returned ACCESS_DENIED; unlock and repeat native and two-device text/PNG testing. No backend-enabled line was found: focus logging is debug-triggered and the durable default logger does not persist debug. No runtime performance acceptance was performed.
- Known limitation (accepted as residual risk in design review, not by the user): macOS `changeCount` check and `clearContents` are not atomic. A local clipboard change in that window can be cleared and replaced by the remote content; the post-write check cannot restore it. Do not claim this risk was eliminated or that the coincidence is impossible.
- Mac backup requires complete app replacement, not overlay; ad-hoc signatures may invalidate Accessibility. Windows install script initially falsely concluded stopped processes remained, then falsely timed out after five seconds of startup; it now polls exact install paths and waits up to 60 seconds. The retry did install, and later independent process/hash checks succeeded. Preserve both install logs; no automated rollback or cross-machine atomicity is provided.

## 2026-10-29: Windows clipboard sequence drift after native write

- Completed `target/codex-tasks/win-generation-drift.md` using the non-deterministic drift option. A real Windows trace measured text as `before Empty=1926`, `after Empty=1927`, `after SetClipboardData=1928`, `before CloseClipboard=1928`, and `after CloseClipboard=1931` (close delta `+3`). PNG measured `1931 -> 1932 -> 1933 -> 1933`, with close delta `0`. The trace confirms the post-close number is not a stable ownership receipt; it does not by itself distinguish synthesized formats from a clipboard listener.
- Generation-fenced Windows writes now read back the exact native payload with `GetClipboardData` while `OpenClipboard` is still held. A readback mismatch fails the write; the returned generation remains the in-lock value. This avoids attributing the later `CloseClipboard` sequence increments to the write while still confirming the locked content is ours.
- The ignored real `clipboard_focus_native` text+PNG test now asserts `written <= generation()` and passed on the unlocked interactive Windows desktop. The focused fake-backend regression models a write at generation 1911 followed by a `+3` post-close drift: the generation miss causes one fresh read, the content hash matches the known source content, and no echo is sent back.
- Verification passed: `cargo fmt --all -- --check`; `cargo test --workspace --all-features --locked --target-dir target/codex-race` (451 passed, 5 ignored); strict all-target/all-feature Clippy (0 errors, 3 existing warnings); and `git diff --check`.
- The separate Windows measurement test passed and printed the sequence trace above. No commit, install, deployment, or permissions change was performed.

## 2026-10-29: two-machine acceptance

Installed release, Windows unlocked, Mac user-reported Accessibility granted. Mac Sync reported CaptureStatus=Enabled and EmulationStatus=Enabled; SSH ax-probe was not used. Windows frontend/daemon PIDs 87108/83848 and Mac frontend/daemon PIDs 63650/63654 remained running. Bilateral clipboard grants were enabled via IPC and confirmed. This run stopped at the task's explicit manual-crossing exception; it is not a complete acceptance.

| Case | Result | Timing / evidence |
| --- | --- | --- |
| 1 Windows text to Mac | PASS, observed content | `acc-win-382a0f2e1c` matched `pbpaste` after Windows DPI-aware edge injection at (2520,1200), 80 moves and a 3 s wait. The total command included about 2 s of input and 3 s waiting; exact transfer latency was not measured. Mac cursor subsequently responded to Windows motion. |
| 2 Mac text to Windows | INCONCLUSIVE: automated return not observed | Mac `pbcopy` = `acc-mac-6f70744f66`; Windows remained `acc-win-382a0f2e1c` after 180 leftward moves and again after 250 moves over about 3.75 s plus 2 s wait. Mac cursor moved from x=1469 to x=0 at y=715; Windows cursor remained at (2559,1200) on prior check. Additional Mac CGEvent posts did not establish a return. This is not proof of a clipboard failure because actual return was not demonstrated. |
| 3 No echo | NOT RUN | Depends on successful case 2 and a real return/recross. |
| 4 2 MB PNG, both ways | NOT RUN | Repeatable cross/return not established. |
| 5 15 MiB PNG | NOT RUN | No valid crossing-to-paste timing measured. |
| 6 Revoke and restore | NOT RUN | No reliable cross/return; both grants left enabled. |
| 7 Restart and reconnect | NOT RUN | Must repeat a confirmed cross after restart; no restart performed. |

Relevant logs, UTC; neither excerpt proves the reason for the failed simulated return:

- Windows `%LOCALAPPDATA%\Mousehop\logs\connection-daemon.log`, repeated every ~3 s around 00:24-00:25: `no input-hook callbacks during active capture — hooks presumed removed by the OS; re-installing`, followed by `re-installed low-level input hooks`.
- Mac `~/Library/Logs/Mousehop/connection-daemon.log`: `[2026-10-29T00:21:29Z WARN mousehop::transport::driver] KCP DTLS receive ended: Err(Std(StdError(ErrAlertFatalOrClose)))`; at 00:21:41Z `refusing 192.168.180.134:59669: peer did not complete the mousehop handshake within 12s — closing connection`. These messages concern an earlier connection attempt, not an established cause of the return problem; the task notes normal installed stdout/stderr are discarded. The 2026-10-28T16:20Z Accessibility warnings predate the granted permission and restart and do not establish the current permission state.

Manual continuation needed: with the installed apps open, physically move the Windows mouse from the Mac screen back across its left edge to Windows and observe whether the Windows pointer returns and Mac text appears. If it does, perform case 3 immediately, then 4-7 in the task with actual crossings, record timings and both logs; if not, capture timestamped current debug evidence before concluding a product defect. No source, installation, or TCC changes were made.

### Windows wfix installation for subsequent acceptance (reconnect pending)

Windows switched to the wfix build from `target/release-pipeline/clipboard-release-20261029-082844-019639/`. The previous case 1 PASS above applies only to the earlier build; cases 1 onward must be rerun against wfix and are not yet accepted. Installed EXE hashes match the stage, launcher/frontend/daemon are running, and bilateral grants read true. Windows IPC still showed Mac `alive=false`; `LogonUI.exe` was present in the daemon's console session during attempted reconnect. Unlock Windows and verify a real reconnect before continuing dual-machine cases. Full package, backup, script false-negative and evidence details are in `target/release-pipeline/README.md`. No Mac configuration or processes were changed.

### 2026-10-29 08:55: wfix reconnect continuation blocked by lock

- At 08:54:40 local, `Get-Process LogonUI` returned no process. Windows daemon 90924 Sync reported Mac `alive=false`, `active_addr=null`; Windows grant for Mac was true. Mac daemon 63654 Sync reported grant for Windows true and an empty outgoing `Enumerate`: Mac's incoming connection cannot be represented as an outgoing `alive=true` in that list.
- At 08:54:58 local, `LogonUI.exe` PID 83848 appeared in console session 1, the Windows daemon's session; reconfirmed at 08:55:12. Windows therefore relocked during verification. The task prerequisite no longer holds; no further simulated cross-screen acceptance or daemon restart was performed, and cases 2-7 remain pending. Ask the user to unlock and keep the session unlocked before restarting the Windows service and continuing.

### 2026-10-29 09:00: wfix restart and case 2 manual crossing needed

- Windows was unlocked; restarted only the exact installed launcher/frontend/daemon. New PIDs 91156/95816/83112. Windows IPC after edge movement reported the Mac peer `alive=true`, `active_addr=192.168.181.213:4252`, `transport_status=Legacy`, display 1470x956; repeated at 08:59:58. Both local ClipboardDeviceGrants snapshots remained true. Mac daemon 63654 had an empty outgoing Enumerate and UDP 4252 listener. A Mac IPC observer did not capture a DeviceConnected event; therefore the Mac-side incoming connection is not independently verified in this run, despite the Windows alive snapshot. Mac 00:59:00Z/00:59:12Z log warnings show a separate incomplete DTLS handshake and are not proof about that live connection.
- Case 2 under wfix: Mac clipboard set and checked `acc-mac-07264991b7`; 180 Windows leftward synthetic moves over ~3.6 s and 2 s wait left Windows clipboard at prior `acc-win-382a0f2e1c`. Mac CoreGraphics read cursor (0,0). An actual return was not established, so clipboard result is inconclusive, not a product failure. Windows remained unlocked at last check. Cases 3-7 pending. User needs to physically move the Windows-controlled pointer from the Mac screen back across its left edge to Windows, then report whether the pointer returned; preserve the prepared clipboard and do not copy anything else. If it returns, read both clipboards and immediately do case 3 before any further copy.

### 2026-10-29: wfix Windows-to-Mac partial cases 4-7

Case 2's prior Mac text `acc-mac-07264991b7` was independently read and documented before the clipboard changed. Mac now contains PNG, so case 2 must be freshly prepared before a manual return; case 3 must immediately follow its successful return.

| Test | Status | Evidence and limits |
| --- | --- | --- |
| 4, Windows to Mac ~2 MiB PNG | PASS for content transfer; crossing attribution incomplete | Windows `target/diagnostics/dual-wfix-20261029/case4.png`: 724x724 RGBA, 2,212,810 bytes, SHA-256 `0c639a10cb8b06551fef9f46357fb458a7b32c4843b68457f154b19c16d93a42`. After simulated release chord/cross, Mac still showed text at 09:02:03; by the next observation at 09:02:35, following a Mac app/daemon restart, `clipboard info` showed PNG 2,097,826 bytes. Read `NSPasteboard` PNG via Swift and compared decoded RGBA against Windows source: exact 724x724 pixel match; received encoded SHA-256 `504aae5b9f7d4229682d3a7b30edcc95e896ca23f3b7944d257e42dd7dcf3160`. Exact arrival time and whether arrival preceded or followed restart were not observed; not proof of the required crossing timing. Reverse direction not run. |
| 5, large PNG Windows to Mac | NOT ACCEPTED; crossing not established | Windows 1982x1982 RGBA PNG 16,579,673 bytes (15.81 MiB), SHA-256 `7cef843a35d880ee2eadb96a787acb3591951c85362aadb04ce1cde659616fe7`. Following simulated chord/cross, Mac still held the previous 2,097,826-byte PNG at 09:03:35 and repeatedly until 09:04:02. The first timing poll failed with a Windows GBK console encoding error; no valid crossing-to-paste latency is available. Without proof of new focus crossing this is inconclusive, not a demonstrated large-image product failure. |
| 6, grant revoke/restore | NOT ACCEPTED; no verified crossings | Windows grant changed false; a fresh later Sync returned `{}` (missing grant means denied). Windows text `acc-revoked-wfix-973ac5` and a simulated cross left Mac's 2,097,826-byte PNG unchanged. The initial IPC stream included an earlier true snapshot, so the exact ordering of grant application versus simulated crossing is not proven. Grant restored and independently re-read true; new Windows text `acc-restored-wfix-2a9d73` plus simulated chord/cross did not appear on Mac after 5 s or an additional 8 s. No genuine return and re-entry were established, so neither blocking nor restoration can be accepted. Mac grant remained true. |
| 7, Mac daemon restart + reconnect + text | PARTIAL, NOT ACCEPTED | Stopped exact installed Mac frontend/daemon 63650/63654, relaunched `/Applications/Mousehop.app`; frontend 73329, daemon 73337. Mac IPC CaptureStatus/EmulationStatus Enabled and grant true. Windows IPC subsequently reported Mac `alive=true`, address `192.168.181.213:4252`, Legacy, including at 09:05:27. Independent Mac inbound event not captured; subsequent restored-grant text did not reach Mac under unverified simulated crossing. Thus reconnect on Windows passed, complete case 7 did not. |

Relevant logs, UTC: Windows at 01:05:01-01:05:16 repeatedly `no input-hook callbacks during active capture ... re-installing` / `re-installed low-level input hooks`; Mac at 01:02:42 `KCP DTLS receive ended: Err(Std(StdError(ErrAlertFatalOrClose)))`, at 01:02:54 `refusing 192.168.180.134:62713: peer did not complete the mousehop handshake within 12s`. These warnings do not establish the cause of missed focus or transfer. No source, install or TCC changes. At last check Windows was unlocked and bilateral grants were enabled. Manual repeatable return/crossing is still required before remaining acceptance can be attributed to clipboard behavior.

### 2026-10-29: case 4 Mac-to-Windows PNG diagnosis (read-only; root cause not yet isolated)

- User reports that the 09:44 Mac copy of `/tmp/acc4.png` was followed by a real Mac-to-Windows return; Windows still held the prior text at 09:46. User reports Mac-to-Windows text and case 3 no-echo passed; these are user observations, not independently reconstructed from transfer logs. At 09:50:23 Windows sequence 1974 had formats 13/16/1/7, no registered PNG, DIB or DIBV5. This confirms the PNG had not appeared by that observation, but is not itself proof of a write attempt.
- Mac live pasteboard read-only check: `changeCount=346` before and after; native types include `public.png`, `Apple PNG pasteboard type`, `public.tiff`; `public.png` is 2,098,131 bytes, byte-identical to `/tmp/acc4.png` (file SHA-256 `bad1fbb02a2bac59312ed178c5ceaa5bebf2a51b2ed3372fa50ea1ba45696fa3`). No clipboard was written or cleared. At 09:50:54 a separate read-only Rust probe linked the actual `mousehop-input-capture` release rlib from the Mac build of the installed binary (installed daemon SHA-256 `a4512cd377822ada3c39b69d9559bbc19f7c7975ec953fe772f19a9a6ddf12`). It called the same `read_current_sync` with an empty suppression set: `generation_before=346`, frontmost `MacBundle("com.netease.uuremote")`, result `Png`, 2,097,826 canonical bytes, `generation_after=346`. The Mac config had no `clipboard_suppress_apps` entry, consistent with an empty configured list. Therefore the observed current payload passes concealed-type/privacy check, native `public.png` selection, actual `decode_png` and re-encode, and 16 MiB/8192-dimension/16M-pixel bounds. This is a separate SSH process and later observation, not proof of daemon's exact frontmost app, suppression lock state, or generation at the original crossing. Probe is `/tmp/mousehop-readprobe-m3PtCF/`; it does not alter the installed app or sources.
- Mac and Windows persistent `connection-daemon.log` files last wrote at 09:44:23 local; installed daemon output is not captured after that. The last Windows line at 01:44:23Z says `releasing capture: peer returned through a transactional release-only edge`; the Mac line at 01:44:23Z says `releasing capture: no active client at this position`. These show a return around that earlier timestamp, not whether the user-reported PNG crossing triggered the incoming focus event. Incoming `CaptureBegin` calls `clipboard_focus_out_incoming` (`mousehop/src/service.rs`); unlike the outbound branch it emits no trigger log. Neither available log proves focus, offer, send, receive, decode, or native Windows write for this PNG; lack of logs is not proof of absence. The native adapter collapses read and write errors to `None`/`Failed`, while focus/session drop paths generally lack reason logs. A Windows write error, generation-fenced stale discard, no incoming lane/trigger, or transport failure cannot be distinguished from existing evidence.
- Legacy describes input transport, not clipboard bulk: negotiated clipboard uses its own KCP-over-DTLS lane (`mousehop/src/transport/driver.rs`, `mousehop/src/transport/clipboard.rs`), with 1200-byte MTU / 1176-byte MSS, 128-segment windows, 10 ms KCP update, 25 ms focus-manager tick, low-priority bulk scheduling and bounded queues. The 2,097,826-byte canonical sample is roughly 32 64-KiB session chunks and 1,784 data segments before framing/retransmissions. A local queue admission/completed_send is not peer clipboard-write acknowledgement. No measured throughput or in-flight progress exists, so a 2-minute-or-more wait and the 09:50 missing PNG make a routine brief delay less persuasive but cannot exclude a stalled or heavily starved transfer; no numerical completion guarantee follows from the configured intervals/windows.
- Source-level reliability/observability risks, not proven trigger: `FocusManager` can cache a `None` sample for the generation; it marks a failed write as known, and a send as accepted after local frame admission, without end-to-end successful OS-write acknowledgement. Recommend first capture synchronized, privacy-safe per-transfer lifecycle telemetry (peer/session and non-sensitive transfer ID, event/focus, generation, format/size, suppression reason, offer/chunk progress, completion, validation, stale/write error and elapsed time) on both installed endpoints. Then repeat this same PNG with a real crossing, monitor progress and Windows generation through completion, and measure effective throughput/input latency. Only after the failed stage is demonstrated, fix that stage; add bounded write-failure retry and receiver success/failure acknowledgement so failed delivery is not permanently recorded as known. Preserve privacy and generation fencing, and do not log clipboard contents. No restart, reinstall, TCC or source changes were made during this diagnosis.
- Follow-up (2026-10-29 ~10:00): the user manually copied real text and images both ways on the installed build and both worked. The cause of the `/tmp/acc4.png` scripted failure was not established; the successful manual transfers do not identify that cause. The proposed diagnostics logging was implemented and then reverted at the user's request.

### 2026-10-29 10:50: advanced build installation and two-machine smoke

- Built and installed both platforms from `target/release-pipeline/clipboard-release-20261029-103730-806005/`, snapshot commit `9992f2c-dirty`, source archive SHA-256 `5db6cc779a26ab567aaa03091d39da98650cddcc16580535c5dbb8b128b62a7d`. Snapshot consistency checks do not guarantee an atomic view during concurrent edits. Pre-build `cargo test --workspace` and Go 1.27.3 `go test ./...` passed; logs are in the run directory. Updated the `scripts/test-mygo-lan.py` expected IPC version from 7 to 8; the LAN test itself was not run.
- Before installation, backed up all 2,750 Windows installation files to `backup-windows/` and verified their hashes. Mac pre-install backup is `~/.cache/mousehop/clipboard-release-20261029-103730-806005-preinstall-backup/`; both executable copies were compared, and existing config was copied when present. Mac installer also preserved the original app in `~/.cache/mousehop/clipboard-release-20261029-103730-806005-backup/`. Config was not deleted; no TCC changes.
- Mac installed first, then Windows. Installer SHA-256: Windows `04cda87671668222554546af5b6ce3a61428eb3bd18714cac45aef3c4b41238b`; Mac DMG `a481cc943a677e9525484ddf0e611c654f64380c40602b96b0562dc8661e82ed`. Installed binaries match their respective build stages: Windows daemon `870a966b39c2f426b4d9ee187912b5f6092d469834b170876119a24eb4e8290f`, frontend `e8b0443f9758bc7c5362effe20fe0b6544911dee2807daae22ed5e14de7a2925`; Mac daemon `f74f4112af473ce573d1d1b3702ab878204d94e6046d27dc3073618d518d9308`, frontend `a9f776d8ef5417b9483f4d4dc35e77e4da7cb10a9f5fabbf73a3cb41fd3ff7cc`. Both installed frontend binaries contain `9992f2c-dirty`; About page rendering was not inspected.
- Mac installer exited successfully. Windows installer script returned exit 1 `Frontend not observed` because its forward-slash path comparison does not match the CIM backslash path; independently observed installed launcher/frontend/daemon PIDs 96100/94736/93228 and both staged hashes matched. Mac frontend/daemon PIDs 5277/5280 observed. Both daemons report IPC 8, CaptureStatus/EmulationStatus Enabled, GlobalKcp=true, ClipboardSync=true and the other peer's clipboard grant true.
- After simulated Windows edge movement, Windows IPC reported Mac `alive=true`, `active_addr=192.168.181.213:4252`, transport `KCP`. Mac's outgoing `Enumerate` remained empty, so its IPC did not independently report an `alive=true` peer; however, a fresh unique Windows clipboard text set before the same simulated crossing matched Mac `pbpaste` after 5 seconds. This independently demonstrates receipt on Mac for this smoke test, not a measured latency or proof of reverse direction. Evidence: `windows-sync-after-cross.json`, `mac-sync-after-cross.json`, `simulate-cross.py`, and installation/build logs in the run directory. Windows was unlocked at the last check.
- This is packaging, installation and one simulated Windows-to-Mac text smoke, not complete real-user dual-machine acceptance, image/reverse-direction coverage, performance or release signing/notarization. Prior wfix acceptance results do not certify this build. The previously accepted macOS clipboard check-to-clear race remains a residual risk.

## 2026-10-29: global Advanced settings and About commit

- Native frontend IPC is version 8. `global_kcp` and `clipboard_sync` default true when absent; legacy `input_transport`, per-client `use_kcp` and clipboard grants remain readable but do not authorize or select the new global policies. Outbound KCP changes apply on the next connection, not to an established session. Natural scrolling and outgoing momentum default true when absent; explicit false is persisted.
- Clipboard lanes are one-shot: disabling detaches them, and re-enabling closes/reconnects sessions to obtain fresh lanes. Admission checks the authenticated fingerprint against the direction's current trusted identities. A native write already admitted to a background worker before revocation cannot be retracted by detaching the lane; this change does not establish stronger native write revocation or dual-machine acceptance. The existing Mac-to-Windows PNG diagnosis remains open.
- Release snapshots record the short source commit plus `-dirty` before copying; Windows and macOS pipeline scripts inject it into the Go frontend. Mac packaging rebuilds and re-signs the Go binary in the isolated app bundle. No installer was built and neither packaging path was executed in this task.
- Windows validation: Go 1.27.3 `go test -count=1 ./...` passed; `cargo test --workspace` passed 454 with 5 ignored; `cargo fmt --all -- --check` passed; workspace all-target/all-feature Clippy passed with 3 warnings. These are not native UI or dual-device clipboard acceptance. The system `go` is too old for go.mod; use the cached Go 1.27.3 toolchain.
- `scripts/test-mygo-lan.py` still requires IPC version 7 and therefore rejects version 8; it is outside the allowed file list for this task and was not edited or run. Update that fixture separately before relying on it.

## Go organization and manual connection dialog

- Separated executable main from internal/app, model, ipc and locale. Version/commit linker flags remain main.version/main.commit. UI pages split into devices_view and settings_view.
- Moved address connection from Advanced into the Nearby header + button, with a modal, explicit Cancel, trimmed address, integer port range 1..65535 and offline guard.
- Windows Go tests and vet passed; Darwin ARM64 cross-build and vet passed. This does not claim native Mac runtime acceptance. No packaging or publication performed for this change.
- Platform pitfall: gopls rename on Windows misses Darwin-only references; verify Darwin compilation separately. Icon generation and fixture paths must follow internal/app relocation.

## 2026-10-29: MotionDuo branding review (not released)

- First double-screen flowing SVG and the monochrome tray SVG are canonical in `assets/brand/`. Raster, ICO and ICNS derivatives are generated from these, not from upstream artwork. `go generate` needs `rsvg-convert`; ICO generation uses Windows PowerShell and ICNS generation uses macOS `rsvg-convert`, ImageMagick and `iconutil`. The ICO 256px frame must reuse the frontend PNG directly: GDI resizing changed edge pixels enough to fail artwork consistency checks. Regeneration on an isolated Mac produced identical SHA-256 for both embedded PNGs and the 512px PNG; ICNS was round-tripped through iconutil.
- Keep `Mousehop` as the MyGo single-instance lock name and legacy bundle ID, binary/config/trust paths and Windows installer AppId/directory; set the native display name after acquiring the lock. This preserves the intended key for matching legacy packaged instances, not a proven universal cross-version guarantee. The macOS bundle filename remains `Mousehop.app`. Upgrade behavior, existing Windows shortcut cleanup, native UI appearance and two-device operation still need real acceptance; no install or TCC change was performed here.
- About and README identify the upstream projects and GPL-3.0-or-later; `LICENSE`, `NOTICE` and third-party notices remain intact. Existing historical screenshots and release names are not retroactively rebranded. The earlier scripted PNG failure's cause was not established; do not characterize it as a proven test-harness artifact.
- No commit, staging, packaging, installation, tagging or push was performed for this review. Windows Go tests/vet and Darwin ARM64 cross-build passed, as did isolated native Mac Go tests/vet/build. Workspace fmt and all-target/all-feature Clippy passed (three pre-existing Cargo warnings). Full Rust test and executable icon-resource checks are reported separately with their actual outcomes in the review response.
- Windows Rust workspace tests completed: 454 passed, 5 ignored. A debug Windows build with `CARGO_TARGET_DIR=target/codex-race` placed the Rust executable there while the MyGo helper explicitly placed the frontend in `target/debug`; both PE icon resource groups matched all seven MotionDuo ICO frames. The helper's generic output message does not reflect the overridden Rust target directory. These build checks did not run an installer or establish native UI acceptance.

## 2026-10-29: MotionDuo 1.1.0 committed, Mac permission blocker

- Commit `63cb2d09889e1f56a8fb14fe47fe99a447cbf8ca` contains the approved product changes and Claude Code co-author trailer. `HANDOFF.MD`, target, secrets, logs and the pre-existing historical screenshot deletion were not committed. No push or GitHub release.
- Committed-source run `target/release-pipeline/clipboard-release-20261029-124940-617840/`, archive SHA-256 `c47f1794ab403575b900a435d38e0786586fc7f72d3528087dcaa93c53a94e25`. Mac build derives shell scripts by recorded CRLF normalization. Windows package `MotionDuo-1.1.0-x64-Setup.exe` SHA-256 `027db1773f73030f77f7120bf4a39d5839ef370b439fa63ef5ce9d8afd20d01f`; Mac DMG `MotionDuo-1.1.0-macOS-arm64.dmg` SHA-256 `5bfcc60f83dbd74ec8926ccb70a63a40b21350c0a1ae44c6f08f5634edd0f271`. Both builds passed architecture/version checks; Windows both PE icons match the chosen ICO. Mac ad-hoc signature and DMG validation passed. Packages are not production signed/notarized.
- Windows legacy-shortcut ownership logic compiled with official Inno 6.7.4 and passed isolated install: matching target/empty arguments removed, matching target with arguments and unrelated target retained. Actual upgrade not yet run. Windows installation backup at the run's `backup-windows/`; actual install and identity backup not yet run. Its current 1.0.0 processes remain in place.
- Mac installed first; installed binaries match the DMG stage (mousehop `c988b35873fc8a4f92d4a86b07f82261638b681307aee589e242a01c9a9b3f6c`, frontend `c294a4d7e529565aab6cd2c3eb406e140c0e3707491442cfe91d420501b01379`), frontend contains commit `63cb2d0`. App and config backup `/Users/alvin/.cache/mousehop/clipboard-release-20261029-124940-617840-backup/`. Config and PEM hashes remained identical through startup; no TCC changes were made.
- BLOCKED: Mac IPC reports 1.1.0, IPC 8, KCP and clipboard enabled, emulation Enabled but capture Disabled. At 12:57:13 local the daemon reported accessibility permission is required and no capture backend. Stop before Windows install as instructed. User must manually grant/re-enable Accessibility for the installed app in macOS System Settings; then confirm and resume. Do not claim connection or native acceptance. After authorization, recheck Mac capture, then run Windows install script against this run, verify shortcuts, identities, installed hashes, versions, IPC and connection. Do not rebuild or overwrite this run without need; do not push/release.
- The scripted PNG failure cause remains unconfirmed; the user previously manually confirmed text and images both ways on the earlier build. That does not establish acceptance for this newly installed build.

## 2026-10-29: MotionDuo 1.1.0 installation and smoke continuation

- User confirmed Mac Accessibility authorized; installed-app IPC, not an SSH ax-probe, showed Mac daemon 48608 version 1.1.0 / IPC 8 with capture and emulation Enabled. No TCC changes or app restart were performed in this continuation.
- Before Windows installation, both old installed binaries matched the existing `backup-windows/`; config.toml and mousehop.pem matched the pre-install hashes. Installed the previously built `clipboard-release-20261029-124940-617840` Windows package, without rebuilding. Installed launcher SHA-256 `02da7afdf03e668c0b49e0d62311e3855c481ebaa906bc4124e1f2428172c01a`, frontend `b00b7dd240b7e731defac5d7541526a3c76b5697d21343064e72aaabceef2076`, both match stage. Launcher reports 1.1.0, commit `63cb2d09`; frontend reports MotionDuo 1.1.0 and contains `63cb2d0`. Launcher/frontend/daemon PIDs observed 44664/73852/77424.
- Both Windows config.toml and mousehop.pem matched their pre-install hashes and private `backup-windows-config/`; Mac config and PEM matched their pre-install hashes. The three old Mousehop shortcuts, previously verified to target this installation with empty arguments, were removed. Start Menu and desktop MotionDuo shortcuts and its uninstall shortcut target this installation.
- Both installed IPC endpoints reported capture/emulation Enabled, GlobalKcp and ClipboardSync true. Windows IPC after synthetic crossing showed Mac alive with active address and transport KCP; Mac outgoing Enumerate is empty and does not expose incoming peer state in that list. Windows->Mac and Mac->Windows unique text matched after simulated crossing/return. Windows->Mac PNG (256x256) matched decoded RGBA pixels after synthetic crossing. An earlier Windows->Mac PNG attempt did not observe PNG; a retry following an observed return did match.
- Mac->Windows scripted PNG: source PNG was confirmed on the Mac pasteboard before synthetic return, Windows logged a transactional release-only edge at 13:47:51 local, but registered PNG was not observed on Windows after 8 seconds or after a further 25 seconds. This is a failed/inconclusive scripted smoke observation, not a proven root cause or a claim that real use fails. During synthetic capture Windows logged repeated no-input-hook-callback/reinstallation warnings; their relationship to the image result is unconfirmed. No changes to source, configuration or TCC were made to investigate.
- User subsequently stated they tested on real devices and found no problem, and explicitly asked to stop testing. Record this as user-reported physical acceptance, without inventing individual tested cases or treating the scripted PNG result as explained. No further tests or restarts were performed after that request. The earlier historical scripted PNG failure also remains unexplained.
- No source changes, commit, push, publication, disk cleanup, or TCC modifications in this continuation. Existing backups and run artifacts retained. Windows installer is unsigned; Mac app is ad-hoc signed and not notarized. Git's pre-existing HANDOFF modification and historical screenshot deletion remain outside the release commit.

## 2026-10-29: MotionDuo process rename (INCOMPLETE; do not release or install)

- The process-rename task supersedes release ordering. `6euisip` explicitly stopped and was idle; only `alvin` was fetched, with remote main `86c0c2f4c6016c1a5055f56681a89f73b984567c`, mygo `9992f2c60fdefdd3520a26226484cf25ed62b0f8`, and no remote `v1.1.0` tag at the check. No push, commit, merge, tag, package or install was performed. Preserve the pre-existing staged README/screenshots, unstaged NOTICE and this handoff.
- Partial edits rename the Rust bin to `motionduo`, Go frontend/child path to `motionduo-ui`, adjust Windows resources, app bundle, build scripts and selected docs/CI/package paths; legacy config, IPC and bundle ID remain unchanged. Windows installer switches shortcuts and has an unvalidated exact-command Run value migration that leaves StartupApproved untouched. `CloseApplications=no` avoids broad automatic process termination. These edits are NOT a verified upgrade.
- Critical unfinished: design and test macOS old/new app relocation and login-item migration (SMAppService on macOS 13+, LaunchAgent on macOS 12); maintain disabled/approval state without starting old app or modifying TCC. Windows must prove process shutdown/path ownership, backup/retirement of legacy executables, startup migration and foreign shortcuts in isolated fixtures, including failure recovery. Existing local `target/release-pipeline/install-mac.sh` and `install-windows.ps1` are deliberately fail-closed pending that work; do not bypass their guards. Current Windows installer does not safely retire leftover old binaries. Do not claim this phase is ready for main review.
- Windows Go test/vet and Darwin cross-build passed before adding the backend-path regression; native Mac Go test/vet/build passed against an isolated tracked-file snapshot before that new test. Rust workspace test passed 454 with 5 ignored; fmt check and workspace all-target/all-feature Clippy passed (3 warnings). These are not migration, real process-name, package or installation acceptance. Repeat tests against the final tree, including untracked regression test, and document actual results. No live user settings or installations were changed.
- Isolated Mac test snapshot `/tmp/motionduo-rename-check-20261029/` intentionally excludes untracked files. The release snapshot script archives only HEAD; never use it for the dirty changes as if it represented this task. Recheck remaining scripts, README historical context and release workflows before any publication.
- Follow-up validation on the current Windows Go tree (including `backend_path_test.go`): `go test ./...`, `go vet ./...`, and Darwin ARM64 cross-build passed. `cargo build -p mousehop --bin motionduo --release --locked` passed with three existing workspace warnings; Cargo metadata confirms the bin target is `motionduo`. Windows GUI-subsystem version output and live process names have not been independently verified. The Mac native snapshot predates the added regression test.

## 2026-10-29: process-rename continuation, still blocked from installation

- Added `PrepareToInstall` to the standalone Inno script to reject an old binary in the selected install directory before file installation. This is only a partial guard: it has not been compiled or exercised, does not discover every alternate installation path, and is not an upgrade implementation. The target install-script fail-closed guards remain in place.
- `target/release-pipeline/windows-upgrade.ps1` remains an isolated prototype, not called by the installer. Added ancestor reparse checks and verified retirement recovery with destination-conflict refusal. `target/codex-tasks/test-windows-upgrade.ps1` passed on fixture `fixture-c2ebaed705164c9fae722955ba736012`, including path filtering, junction rejection, backup mismatch, injected retirement failure, post-retirement restore, and foreign destination preservation. These are not native process-identity/PID-race, registry, shortcut, installer-transaction or integration tests.
- Snapshot `target/codex-tasks/process-rename-snapshot-20261029-141104` includes the untracked backend-path test; Mac `/tmp/motionduo-rename-141104` verified 318 file hashes. Native Go test and vet passed; frontend built. Windows Go 1.27.3 test and vet passed. The initial Windows system Go 1.18.4 failed parsing go.mod; use cached 1.27.3. Direct native Mac `motionduo-ui --version` printed `MotionDuo development`.
- PITFALL: generating the Mac test script with Windows `Set-Content` left CRLF; its last command passed `--version\r`, not `--version`, so the isolated frontend started instead of performing only the version check. Exact isolated process PID 56655 was stopped after verifying its path. No installed process was intentionally stopped; do not assume this accidental UI launch had no side effects on shared application state. Normalize scripts to LF before remote execution. A subsequent direct version command succeeded.
- Mac app/login-item migration and Windows integrated process ownership, transaction, recovery, startup and shortcut tests remain unimplemented; no release readiness claim. SMAppService `mainAppService` status is associated with its calling app, so a separate helper is not proof of the old bundle's registration state. Ambiguous registrations must be detected and the old app retained, not reported as migrated. No TCC changes, installer package, installation, commit or push were performed.

## 2026-10-29: Windows installer upgrade integration (review required)

- `packaging/windows-upgrade.ps1` and `packaging/windows-upgrade-run.ps1` are new untracked source files: include both in any future snapshot/commit. The actual Inno Setup invokes prepare/commit/rollback and no longer rejects every legacy binary. `scripts/package-windows.ps1` and the ignored local release build script generate a stage SHA-256 manifest. Installed process paths are checked exactly; Setup asks the user to close owned processes rather than killing by name. Backups and transaction manifests are retained under the per-user Mousehop upgrade-backups directory. Run values are migrated only for the exact legacy command; StartupApproved is not written.
- Inno 6.7.4 compiled both the isolated-fixture and production preprocessor configurations using fixture payloads. `scripts/test-windows-upgrade.ps1` passed at `target/codex-tasks/windows-upgrade-2f5543b9e59b4e84bc70224bb3eeee45`: upgrade and clean install, owned running process rejection, foreign same-name process retained, backup contents, foreign startup and foreign executable untouched, disabled StartupApproved bytes retained, failures after retirement and startup migration restored old files and startup. The fixture uses a temporary HKCU subtree and disables real Inno shortcuts and uninstall registration.
- IMPORTANT limits: fixture uses fake executable contents except a copied system cmd process, not the release payload; it does not verify Inno's real shortcut/uninstall registration effects or their rollback. Inno can create shortcut/uninstall metadata before ssPostInstall; current PowerShell transaction only backs up stage files, legacy binaries and matching Run values. Thus metadata rollback has not been demonstrated, and this is not complete production rollback acceptance. No real installation, Mac work, TCC changes, commit or push; the local `target/release-pipeline/install-windows.ps1` still has its fail-closed guard.
- Debugging pitfalls: `ConvertFrom-Json` arrays must be enumerated before selecting manifest entries in Windows PowerShell 5.1; `Get-Item` returns a read-only RegistryKey, so use `Set-ItemProperty`. An exception in `CurStepChanged` alone did not produce a failing process exit; use `GetCustomSetupExitCode` for commit failure. The installer tests expose PowerShell errors through the Setup log. Stage manifests must be generated from the same staged bytes given to ISCC.

## 2026-10-29: clean-install scope supersedes migration

- User cancelled automatic upgrade and login-item migration. Removed the new Windows migration scripts and fixture, their Inno transaction and manifest integration, and the local migration prototypes; retained product renaming and existing user changes. Windows Setup refuses to install into a directory containing the old executables and does not automatically close applications. This guard only covers the selected directory, not all possible old installations.
- Inno 6.7.4 compiled the production configuration with fake payload at `target/codex-tasks/clean-installer-compile-150f222c134e4435bfb70c65823a0d71`; do not install this fixture. Windows Go tests and vet passed; `cargo test -p mousehop --lib --locked` passed 254 tests with 2 ignored; PowerShell packaging scripts parsed and `git diff --check` passed. No real install/uninstall, commit, push, release, or TCC change. Local install scripts remain blocked and are obsolete; do not bypass them.
- Before any later uninstall, back up and verify the exact installed application and user config/certificate. The current per-user uninstall registration points to `%LOCALAPPDATA%\Programs\Mousehop\unins000.exe`; confirm AppId, path and command again before use. The 63cb2d0 installer source installs files only under `{app}` and has no explicit config deletion, while the default config and certificate are under `%LOCALAPPDATA%\mousehop`; this does not independently prove the contents of the installed uninstall log. Preserve backups and verify actual user data after uninstall. Do not enable disabled login items or alter TCC; user review is required before any uninstall/install.

## 2026-10-29 MotionDuo 1.1.0 release continuation

- Product commit `495822c252cc5fb5bd86266dd5bb551bca8f803f` was pushed to `alvin/mygo`; normal merge `46e60d547d4e7805b0a1a2742ac7f46b916fbeae` was pushed to `alvin/main`. `origin` was not pushed. No tag or release was created and no actual uninstall/install was run in this continuation.
- Local Windows Go tests/vet and Rust workspace test/fmt/clippy passed; native Mac Go tests/vet/build passed in the isolated verified snapshot with a short TMPDIR. Final-main package run: `target/release-pipeline/clipboard-release-20261029-144519-129745`. Mac DMG built successfully with Rust version commit `46e60d54` and frontend containing `46e60d5`. Snapshot builds must include verified Git metadata: a git archive alone causes shadow-rs to emit an empty commit hash. The source archive itself remains a source snapshot.
- GitHub Actions on the merge failed: Windows Go `TestSupervisorStopsOwnedChildBeforeReturning/false`, `daemon_test.go:131`, helper marker does not belong to supervised child; macOS ARM and Intel `fake_sixteen_mib_transfer_within_eight_ticks`, `clipboard_focus.rs:978`, elapsed 1.637243167s and 1.056019094s; Linux Rust unused imports/dead code; Nix requires Go 1.27.3+; release-plz token missing; workflow drift check failed. Do not describe local passing tests as CI passing. Investigate and resolve failures and rerun before release/install; elapsed-time assertion alone does not prove a production transfer failure.
- Local `target/release-pipeline/install-windows.ps1` and `install-mac.sh` implement backup and clean reinstall, not upgrade migration. They have syntax checks only, not real-install acceptance in this run. They do not authorize TCC changes or enabling autostart. Check exact paths, existing state, and backups anew before use. Previous locally built packages lacking the Git commit must not be released.
- Final-main Windows installer compiled and reports Rust commit `46e60d54`; frontend contains `46e60d5`. Windows installer SHA-256 `faf87fdefcce683acc175425014932436caefa519206bf03a5c0fd4b6248da44`, Mac DMG `0dc760361709850ea09d1978955f7c0476d2c9b11fe8b427a05f12b53ece1f9c`, source snapshot `26239d90e21ddd7300aef84efb97bbc845131d87618e1340626145f6686933d5`. These are local, unpublished artifacts; do not confuse them with a completed release.

## 2026-10-29: Disabled peers in Screen layout

- Devices uses authoritative `State.Active`; `SortedClients` includes disabled peers. Filter layout geometry, rendering and hit testing consistently, but leave the model and Devices list intact. Prune selection, drag and pending layout state on each model event as well as view synchronization, so a disable/re-enable between frames cannot retain a stale gesture or pending ghost.
- Clearing frontend pending state cannot cancel an `UpdateScreenPlacement` already sent to the daemon; any later authoritative placement update is still applied. Windows Go tests and vet passed; native GUI and real-device acceptance were not performed.

## 2026-10-29: Mac file-reference clipboard boundary

- A Mac pasteboard can advertise both `public.file-url` or `NSFilenamesPboardType` and text. Do not fall back to text for those representations: it can expose a local path. The local reader now suppresses that fallback, but this is not a general proof for every file-reference alias or native pasteboard behavior.
- The requested image-file conversion remains unimplemented: local ordinary-file, network-volume and cloud-placeholder eligibility has not been established reliably. Never replace the missing predicate with an extension check or unbounded NSImage conversion; actual clipboard acceptance remains pending authorization.
- After separate SSH authorization, an isolated Mac ARM snapshot of the partial guard passed 5 focused pure unit tests and targeted strict Clippy. This does not exercise the native pasteboard or implement or validate file conversion; see `target/mac-image-file-fix/report.md`.

## 2026-10-29: Go-only side-conflict guard

- The Devices and Screen layout checks are frontend-only. They use the last synchronized client states; they do not prevent backend auto-deactivation when startup duplicates, the CLI, another frontend, or a stale UI bypasses the check. Do not describe this as a backend invariant or independent enablement guarantee.

## 2026-10-29: release build and prerelease pitfalls

- Toolchains: the system Go is too old on both hosts (Windows 1.18, Mac 1.20). Use Go 1.27.3: Windows `~/go/pkg/mod/golang.org/toolchain@v0.0.1-go1.27.3.windows-amd64/bin/go.exe`, Mac `~/.cache/mousehop/toolchains/go1.27.3-darwin-arm64/toolchain@v0.0.1-go1.27.3.darwin-arm64/bin/go`; pass it as `GO=` to `scripts/build-mygo.sh` with `GOTOOLCHAIN=local`.
- Mac Rust over ssh: `~/.cargo/bin/cargo` is a broken symlink to a removed rustup-init. Put `~/.rustup/toolchains/1.95.0-aarch64-apple-darwin/bin` first on PATH.
- Inject `-X main.commit=<sha>` into the Go ldflags manually. The build scripts only set the version, and archive builds have no VCS info, so the About page would show no commit.
- Mac builds: transfer `git archive <commit>` into a fresh `~/.cache/mousehop/mac-build-*/source`. Scripts from an archive made on Windows can have CRLF line endings (`set: -: invalid option`), so run `tr -d '\r'` on `scripts/*.sh` first. The Mac has no `rsvg-convert` or `magick`, so `makeicns.sh` fails; use the checked-in `assets/brand/motionduo.icns` as `target/icon.icns` and skip that step. `package-macos.sh` refuses an existing bundle, so delete `target/release/bundle` first. There is no create-dmg; use `hdiutil create -format UDZO`.
- `release.yml` skips prereleases, so prerelease assets must be uploaded manually. `gh` is not installed locally; use curl against the REST API with the token from `git credential fill` and never print it. Inno Setup is also missing locally, so the Windows prerelease ships a zip instead of Setup.exe.
- kero: `kero send --enter` may leave the text unsubmitted. Follow it with `kero key <id> enter` and confirm "Working" with `kero read`.

## 2026-10-29: macOS stable signing and TCC

- Ad-hoc signatures have a cdhash-based designated requirement that changes on every build, so each reinstall loses TCC/Local Network grants. Local Network denial looks like `failed to connect ... Broken pipe` in the daemon log and `tcp drop outgoing ... reason: NECP` in the kernel log, even when ping works.
- The Mac has a fixed self-signed identity "MotionDuo Local Signing" (SHA-1 3524276994D9701345AC8402C9F47DD27A29BDBC) in the dedicated keychain `~/Library/Keychains/motionduo-signing.keychain-db`. The keychain password and p12 are in `~/.config/motionduo-signing/` (0700); never print or commit them. Before packaging, unlock the keychain, then run `MACOS_SIGN_KEYCHAIN=<kc> MACOS_SIGN_IDENTITY=<sha1> sh scripts/package-macos.sh`. Grants made after the first install with this identity survive later reinstalls; do not regenerate the certificate.
- Full Disk Access probe: `~/Library/Application Support/com.apple.TCC/TCC.db` may not exist (ENOENT). Probe `/Library/Application Support/com.apple.TCC/TCC.db` and `~/Library/Safari` instead; EPERM means access is missing.
- Known, deferred: after granting Full Disk Access, the System Settings "Quit & Reopen" prompt quits MotionDuo but does not reopen it. Likely cause, from reading the code (not confirmed by testing): mygo's `applicationShouldTerminate:` always returns `NSTerminateCancel` and quits on its own, so System Settings sees the quit as refused and never relaunches. A fix means patching mygo or asking the user to reopen manually.
