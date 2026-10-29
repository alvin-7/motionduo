# macOS Ctrl handoff release repair

## Goal and evidence

Repair remote Ctrl release after Windows-to-Mac handoff without changing the
network protocol, Windows capture, application version, or local configuration.
The captured incident proves that Mac submitted left-Control flags=0 and cleared
its internal held sides, while sampled HID/session state remained held and later
cleanup reported clean. A stale pointer event is a plausible reassertion source,
not yet a proven exclusive cause.

Evidence: target/diagnostics/ctrl-stuck-20261020-194714/observations.md and the
two *-modifiers-repro1.jsonl recordings beside it.

## Execution

1. [x] Preserve the incident; inspect modifier injection, pointer event creation,
   cleanup ownership, and existing retry/test contracts.
2. [x] Retain successful remote modifier releases until observed clear. Reconcile
   HID/session state with a bounded asynchronous release barrier; use the existing
   CGEvent release path to order a release after older CG pointer events. Preserve
   pending ownership on errors and include it in cleanup residue.
3. [x] Explicitly stamp movement, button/drag and both scroll paths with the
   current remote modifier plan plus distinguishable local modifiers, preserving
   unrelated CG flags. Prevent released remote flags becoming a local baseline.
4. [x] Add platform-independent regression coverage for accepted-but-ineffective
   releases, delayed observations, release fences, stale pointer flags, local and
   sibling-side modifiers, re-press, API failure and teardown. Update DOC.md.
5. [x] Run focused/full affected-crate tests, formatting, requested workspace
   Clippy, and a native macOS compile/test in an isolated source directory.
6. [x] Review the diff and record evidence. Keep source/test completion distinct
   from the still-required candidate installation and real dual-machine handoff
   acceptance. Do not commit, publish, replace the installed app or change TCC
   permissions as part of this source repair.

## Constraints

- No busy waiting or blocking sleeps in the Tokio worker.
- Retry/readback is bounded; failure must not be logged as clean.
- Do not release a currently held sibling modifier or replay modifier presses.
- Preserve distinguishable local input; simultaneous same-side local/remote
  ownership is intrinsically ambiguous in the existing combined-state API.
- Do not record ordinary keys, text, clipboard payloads or pointer trajectories.

## Progress

- Added 12 regressions; the final focused run passed all 43 modifier tests.
- Windows workspace tests/Clippy and isolated native macOS tests/check passed.
  Native validation uses the installed 1.95.0 toolchain directly because the SSH
  account's cargo shim is broken; no system toolchain was changed.
- The Windows git archive exported CRLF source; the isolated Mac patch was applied
  with whitespace-tolerant context matching. Final source equivalence passed
  after newline normalization for both changed Rust files and DOC.md.
- Corrected diagnostic artifact labels: CG source 0 is combined-session state,
  source 1 is HID state. Both decisive source samples had identical stuck values,
  so the recorded release divergence is unaffected.

## Final validation

All logs below are in target/diagnostics/ctrl-stuck-20261020-194714/.
The Mac source/build directory is
/Users/alvin/.cache/mousehop/ctrl-release-validation-20261020/.

| Platform | Command | Result | Log |
| --- | --- | --- | --- |
| Windows | cargo test --workspace | 308 passed, 0 failed, 2 ignored | workspace-tests-final.log |
| Windows | cargo test -p mousehop-input-emulation --lib macos_keyboard | 43 passed, 20 filtered | modifier-tests-final.log |
| Windows | cargo clippy --workspace --all-targets --all-features | exit 0 | workspace-clippy-gtk.log |
| Windows | cargo fmt --all -- --check | exit 0 | format-final.log |
| Windows | git diff --check | exit 0 | diff-check-final.log |
| macOS arm64 | cargo test -p mousehop-input-emulation --lib | 65 passed, 0 failed | native-tests-final.log |
| macOS arm64 | cargo check -p mousehop --all-targets | exit 0 | native-check-final.log |
| macOS arm64 | cargo clippy -p mousehop-input-emulation --all-targets --all-features | exit 0 | native-clippy-final.log |

Windows workspace validation used the existing GTK SDK under
target/kcp-tools/gtk-2026.8.0 with process-local PKG_CONFIG, PKG_CONFIG_PATH,
PATH, LIB and INCLUDE settings. Initial attempts without these settings failed
dependency discovery; those logs remain preserved. The two ignored tests need a
real desktop/geometry and are not counted as successful live acceptance.
Cargo reports existing workspace default-features warnings. Native build logs
also include IOHIDPostEvent deprecation and an ar option-probe warning; builds
and Clippy still returned zero.

native-source-equivalence.json records normalized SHA-256 equality for the two
Rust files and DOC.md. Final review covered retained release ownership, timeout
and cancellation, fallback-path settlement, all four pointer posting paths,
same-kind sibling preservation, and multi-modifier cleanup. No changes were made
to Windows capture, transport, versions, installed binaries or configurations.

The source repair and automated validation are complete. Installed applications
remain the previous build; no commit or release was created. A candidate install
and repeated real Windows-to-Mac handoffs, including idle release, motion,
drag/click/scroll and local-modifier coexistence, remain required before claiming
that the reported intermittent issue is eliminated. The filtered diagnostic
collectors remain active; their lifecycle is recorded in observations.md.

## Acceptance scenarios

- Submit remote left Ctrl-up while the sink continues reporting Ctrl held:
  residue remains pending; retries target only that released side.
- An observed zero before a CG release fence does not prematurely retire pending
  ownership. A fence followed by observed clear completes the release.
- Windows Ctrl held at entry, released during motion: movement, drag, click and
  scrolling cannot inherit stale remote Ctrl flags.
- Local Shift and local opposite-side Ctrl survive remote left Ctrl release.
- Re-pressing the remote side cancels its old release obligation; retries cannot
  release it while newly held.
- Failed release/fence or timed-out observation retains residue for cleanup.
- Idle after Ctrl-up still performs the bounded readback/retry; no further mouse
  event is required to trigger repair.
- Native Mac compile/tests pass, followed separately by user-assisted repeated
  Windows-to-Mac Ctrl handoffs and OS-state confirmation with the filtered
  collectors.

## Authorized packaging follow-up

The user subsequently requested release packages after the repair. This extends
the scope to local packaging only, not publication, installation, TCC changes,
version changes or real dual-machine acceptance.

1. [x] Inspect existing scripts, prerequisites and validated source provenance.
   Preserve version 0.17.16 and use ctrl-release-fix artifact names in a new
   timestamped directory so these packages cannot be mistaken for the old release.
2. [x] Build and package Windows x64 with the existing GTK/Inno Setup pipeline.
   Verify executable version, installer contents and SHA-256.
3. [x] Build and package native macOS arm64 in the isolated validation directory.
   Bundle GTK libraries/resources, verify ad-hoc signature and DMG integrity,
   and exercise dependency loading without launching the GUI or daemon.
4. [x] Collect both installers, logs, source patch, checksums and release notes.
   Record packaging success separately from still-pending real handoff acceptance.

Output: dist/ctrl-release-fix-0.17.16-20261020-211009/.
Windows release build, Inno Setup, executable --version and PE x86_64 checks
passed. Native arm64 release build, bundle dependency closure, plist/resources,
strict ad-hoc signature, release-marker, DMG integrity and read-only mounted
bundle checks passed. The transferred DMG hash matches its native build hash.
Source patch and normalized Rust/DOC hashes remained unchanged during packaging.
RELEASE-NOTES.md, manifest.json and SHA256SUMS.txt record details and limitations.

The Windows package is unsigned. The Mac package is arm64-only, ad-hoc signed
and not notarized. No installation or real dual-machine acceptance is claimed.
An independent packaging helper generated the Retina icon assets without
modifying application or tracked packaging source. A later byte-level check
confirmed the tracked icon helper also has valid Retina filenames; redaction
of filename-shaped strings in displayed output had obscured those names.
