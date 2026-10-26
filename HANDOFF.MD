# Mousehop Handoff

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
