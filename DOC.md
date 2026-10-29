# General Software Architecture

## Clipboard focus synchronization

On Windows and macOS, Mousehop synchronizes clipboard text and PNG images up to
16 MiB when focus switches between devices. The feature is bilateral: each
device has its own global Clipboard sync switch under Settings > Advanced, on
by default. Only authenticated fingerprint-trusted devices are eligible; both
devices must have their local switch on before a transfer is accepted. It sends
the current clipboard only for the focus switch that triggers it; Mousehop keeps no clipboard history.

The native adapter is enabled by default on those platforms. Privacy
suppression is checked when a native sample is read, and disconnected,
replaced, untrusted, or disabled sessions are detached before their lanes can be used.
Re-enabling sync establishes fresh sessions because clipboard lanes are one-shot.
Native clipboard permissions and OS-specific clipboard behavior still apply;
this is not a promise of physical dual-device acceptance on every desktop
configuration.

## Clipboard transfer foundations

The internal `mousehop/src/clipboard_transfer.rs` module provides a bounded, frame-at-a-time sender and
receiver for the new clipboard codec. A sender's `frame` can be retried after
transport backpressure; `advance` is called only after successful admission.
Wire offers reject empty payloads and enforce 1 MiB text / 16 MiB PNG limits.
The caller is responsible for authenticated session and consent epochs, local
clipboard generation, cancellation, and serialization of the final context
check with the native clipboard write. The production focus manager supplies
those session and focus-triggered checks. Legacy directional clipboard
transport and controls are disabled;
the new fingerprint-bound per-device grant defaults to off, is not migrated from
old direction flags, and currently does not enable an operational transfer. IPC
version 7 carries consent changes and complete grant snapshots.

## macOS explicit clipboard receipt API (not service-integrated)

On macOS with `input-emulation/clipboard-image`,
`ClipboardEmulation::set_with_receipt_checked(format, bytes, guard, registered)`
accepts `MacClipboardFormat::{Text,Png}` and returns an optional `MacWriteReceipt`.
A single process-wide blocking worker is admitted without a queue; an owned
permit remains with the blocking task even if its async waiter is cancelled.
The lifecycle guard must serialize consent/session/local-generation revocation.
The synchronous registration callback runs under that guard with the receipt and
exact canonical bytes; returning false rejects completion but cannot undo the OS
write. PNG is normalized using the bounded codec. Ownership evidence combines
`declareTypes:owner:`'s returned generation, random marker and exact readback.
It proves an observation, not continued ownership or atomic multi-format writing.
Existing arboard APIs and their unsupported receipt capability query are unchanged.
No service enables this API; native APIs may block or raise Objective-C exceptions.

## Clipboard PNG codec foundation (not integrated)

`input-event/src/clipboard_png.rs` exposes `RgbaImage`, `decode_png` and
`encode_png` behind the opt-in `mousehop-input-event/clipboard-png` feature.
This pure Rust foundation supports the Windows/macOS native image adapter APIs
used by focus synchronization. Peer events and protocol serialization are
unchanged. It reuses the already locked `png 0.18.1`
codec rather than enabling the broader `image` codec set.

The pixel contract is tightly packed, top-down, straight-alpha RGBA8. Native
BGRA, row padding, orientation and premultiplied-alpha conversion belong in
future OS adapters. The codec limits compressed input/output to 16 MiB, each
axis to 8192 and pixels to 16,777,216 (64 MiB RGBA). Decode checks IHDR before
image-buffer allocation and uses a separate best-effort 32 MiB internal decoder
budget, not a hard process-memory or wall-time limit. Metadata text/ICC decoding
is disabled; metadata is not re-encoded. Static palette, grayscale, RGB and RGBA
inputs normalize to RGBA8; 16-bit samples keep the high byte. APNG, bad CRC,
truncation, invalid dimensions and inconsistent RGBA lengths are rejected.
Future async integration must bound concurrency and run this synchronous work
off the input executor; OS clipboard consent and transport limits are separate.

Test with `cargo test -p mousehop-input-event --no-default-features --features
clipboard-png --locked`. Tests cover pixel/alpha roundtrip, color conversion,
bit depths, malformed/truncated/corrupt PNG, APNG, dimensions/pixel budgets,
encoded input limits and output writer bounds. These are portable codec tests,
not native clipboard or end-to-end Windows/macOS acceptance.

The `clipboard-image` adapters expose the native image sampling and checked PNG
writes used by the Windows/macOS focus backend.
Capture consumes the native generation even when privacy filtering suppresses
it, and samples and writes use separate process-wide single-worker limits.
Windows capture accepts bounded PNG and 24/32-bit DIB; macOS capture currently
accepts `public.png` only, not TIFF-only clipboard contents. macOS may allocate
its native NSData before its size can be checked. The checked writer requires a
caller-provided lifecycle guard held through the native write; external OS
clipboard changes can still race this application-level lock. Native APIs may
block, and physical OS permission and dual-device acceptance remain deployment
validation boundaries.

## Events

Each instance of mousehop can emit and receive events, where
an event is either a mouse or keyboard event for now.

The general Architecture is shown in the following flow chart:
```mermaid
graph TD
    A[Wayland Backend] -->|WaylandEvent| D{Input}
    B[X11 Backend] -->|X11Event| D{Input}
    C[Windows Backend] -->|WindowsEvent| D{Input}
    D -->|Abstract Event| E[Emitter]
    E -->|Udp Event| F[Receiver]
    F -->|Abstract Event| G{Dispatcher}
    G -->|Wayland Event| H[Wayland Backend]
    G -->|X11 Event| I[X11 Backend]
    G -->|Windows Event| J[Windows Backend]
```

### Input
The input component is responsible for translating inputs from a given backend
to a standardized format and passing them to the event emitter.

#### macOS capture lifecycle synchronization

Each newly registered macOS capture edge queues a fresh WindowServer lock-state
snapshot before the event tap can emit `Begin` for that edge. This initial
snapshot bypasses transition deduplication: an unlock while disconnected must
not leave the capture task stuck in `Locked` after reconnection. Registration
while locked still publishes `Locked`; reconnecting does not bypass the lock
gate. The four-key forced-release shortcut is independent of normal edge return.

#### macOS receiver keyboard injection

On macOS the receiving side posts forwarded keyboard input as HID-level events:
modifier transitions go out as `NX_FLAGSCHANGED` through `IOHIDPostEvent` with
the physical key code of the side that changed plus the matching general and
device-dependent flag bits, and ordinary keys as `NX_KEYDOWN`/`NX_KEYUP` with no
global flags. This supports Control+Left/Right Arrow desktop switching, which
did not work with the previous `CGEvent` injection path in our Mac tests.

`input-emulation/src/macos_keyboard.rs` holds the platform-independent half: the
side/flag model, side aggregation, default-side materialisation for snapshots
that name no side, release planning and the fallback policy. It is compiled for
test targets on every platform. `input-emulation/src/nx_key_bridge.c` contains
the only SDK `NXEventData` construction and the only `IOHIDPostEvent` call,
to isolate the macOS SDK types and the API deprecated since macOS 11.

`IOHIDPostEvent` replaces the whole global modifier state, so the backend merges
the remote snapshot with observed session flags. These flags include synthetic
input, so simultaneous local and remote presses of the same modifier side
cannot be distinguished reliably. If the IOHID path
is unavailable or fails, the backend records the error, releases the input it
already injected (retrying once through `CGEvent`), and only then continues on
the `CGEvent` fallback, which types text but does not guarantee macOS system
shortcuts. A release that fails on both paths is propagated as an emulation
error: the emulation session stops instead of silently holding the key, and
its shutdown cleanup retries the release.

Remote modifier releases are not considered complete solely because an API
accepted them. The backend retains ownership of released sides, posts an
idempotent `CGEvent` release after older CG pointer events, and asynchronously
checks both HID and combined-session flags/key state. Readback/retry is bounded
to 20 attempts with 5 ms asynchronous sleeps; an unconfirmed release remains in
cleanup residue and is reported as an emulation error rather than `clean`.
This also runs if the sender becomes idle immediately after releasing Ctrl.
Movement, button/drag and both scroll paths explicitly combine the current
remote modifier plan with fresh, distinguishable local modifiers instead of
inheriting potentially stale remote flags from their event source. Unrelated
event flags and a distinguishable local opposite-side modifier are preserved.
Same-side simultaneous local/remote ownership remains ambiguous as noted above.

Native modifier readback keeps side ownership from the device-dependent flags.
The `CGEventSourceKeyState` bitmap is only additional side-agnostic evidence:
after a synthetic right-Shift press it can report the left key code while the
flags correctly report right Shift. Synthesizing a left-side flag from that
bitmap would misclassify the remote alias as a locally held key and reassert it
during release retries. Bitmap-only evidence therefore keeps just the general
modifier bit set until it clears; it cannot invent a side. This applies to both
sides of Shift, Control, Option and Command, and still preserves genuine local
opposite-side flags. Caps Lock and ordinary keys use separate injection paths.

##### macOS keyboard diagnostics

Detailed keyboard logs are disabled by default. To troubleshoot the receiving
Mac, quit Mousehop first and launch its executable with:

```sh
MOUSEHOP_LOG_LEVEL='mousehop::keyboard=debug,info' \
  /Applications/MotionDuo.app/Contents/MacOS/motionduo > keyboard.log 2>&1
```

Logs report permission checks, injection path, modifier transitions, arrow
events, failures and cleanup. `submitted` means the OS API accepted an event,
not that a shortcut ran. Ordinary character keycodes and text are omitted.
Warnings remain in the normal connection logs; debug output requires explicit
redirection as above. No separate diagnostic application is required.

### Emitter
The event emitter serializes events and sends them over the network
to the correct client.

### Receiver
The receiver receives events over the network and deserializes them into
the standardized event format.

### Dispatcher
The dispatcher component takes events from the event receiver and passes them
to the correct backend corresponding to the type of client.


## Network refresh and GUI responsiveness

Network discovery and network-lock fingerprinting share one interface snapshot
per refresh. Blocking OS interface/gateway queries run on a bounded blocking
worker, never on the input executor. Refresh requests coalesce while a query is
running; the service continues dispatching input and IPC until a result arrives.
Unchanged snapshots do not re-register mDNS. Network-policy changes wait for a
fresh result in a bounded queue, rather than blocking input or using an old
network fingerprint. Failed queries retain the last snapshot and reject pending
policy changes, leaving persisted locks untouched.

Listener enumeration also runs off the input executor. Failed enumeration does
not remove existing listeners, and interface events invalidate snapshots started
before those events, so an old result cannot remove a newly added listener.

The GTK frontend writes daemon requests on a dedicated thread through a bounded
FIFO (64 requests). A full queue reports an error rather than blocking the UI or
silently dropping a change. The writer uses
`FrontendRequestWriter::set_write_timeout(Some(Duration::from_secs(1)))`.
A failed/partial write shuts down that IPC stream so the existing reader can
trigger reconnect and resynchronization; queued commands from the old writer are
not replayed. On macOS, closing the main window still hides it rather than
quitting. Activation-policy updates are deferred out of GTK visibility callbacks.
These changes remove blocking/reentrant risks; intermittent native close failures
still require target-machine UI validation.

## Network transport

Peer events and control messages use authenticated DTLS connections over UDP.
There is no separate TCP connection-request protocol in the current data path.
Each accepted or outbound connection has its own local session generation.

The global `global_kcp` setting is controlled under Settings > Advanced and
is on by default. It applies to new outgoing connections; existing sessions
retain their negotiated mode until they reconnect. Incoming connections follow
the authenticated controller. Legacy `input_transport` and per-client `use_kcp`
remain readable but do not choose the outgoing transport.
KCP negotiates a message channel inside the same DTLS connection and never downgrades.
Hello and Ping/Pong stay direct; clipboard uses its own low-priority KCP lane. With both peers on 0.17.7 or newer,
pointer motion also uses direct encrypted UDP; keyboard, buttons, scrolling and
handover/control events share the reliable channel. Cumulative motion and
reliable checkpoints preserve displacement and critical-event ordering. Older
KCP peers retain the all-reliable input channel. Existing business
acknowledgements remain required.

Transport support bit `1 << 2`, controller request bit `1 << 3`, reserved tag 240
and transport version 2 identify the extension without renumbering existing event
tags. Capability alone does not select KCP. A repeated Hello cannot change the
mode of an established session. Legacy controllers retain the old wire format;
the experimental version-1 KCP build must be upgraded on both sides. Offer/Ready
bind a negotiation ID and KCP conversation to the DTLS session. Data frames
include the negotiation ID, and progress frames report cumulative sent and
business-consumed sequence numbers. KCP ACKs alone do not certify consumption.
The top-level `kcp_stall_timeout_ms = 600` setting (integer milliseconds,
1..=60000, default 600) is validated when loading configuration and preserved
when saving it. Invalid TOML settings now fail startup rather than silently
falling back to defaults; invalid live reloads retain the previous configuration.
The service snapshots the timeout at startup for both connection managers,
including future reconnects and listener rebinds. File reloads do not change the
running policy: restart each host after editing its file. There is no wire
negotiation of this value; increasing one host's timeout cannot delay the other's.
This input policy covers sent-but-unconsumed input, local consumption,
the receive gap, KCP DTLS writes and consumption receipts. Remaining receive-gap
age restarts only when the contiguous application receive watermark advances;
catching up clears it. Bootstrap, ACK-only, duplicate or buffered out-of-order
data and Progress alone cannot renew it. Legacy keeps its previous I/O policy.
Peer liveness uses a separate top-level `kcp_peer_timeout_ms = 3000` setting,
also integer milliseconds in 1..=60000, startup-snapshotted on both sides.
Fresh negotiated motion, valid KCP traffic and authenticated Ping/Pong renew only
this liveness clock. Established KCP sessions bypass the Legacy fixed heartbeat
and receive-idle watchdogs, so they cannot override the configured peer deadline.
Longer peer deadlines also delay releasing already-held input after total silence.
Independent bounds remain: 6s transport negotiation, 300ms close cleanup, fixed
queue/window budgets and the existing business handshake timeouts.

Outgoing `ClientConfig.scroll_inertia` defaults to true and is persisted per
device. The controller's authenticated Hello sets preference bit `1 << 4` only
when enabled; KCP's Hello wrapping preserves it. The receiver scopes the opt-in
to the current DTLS session and supplies it to receive post-processing. On a
non-macOS receiver, disabling it drops source momentum, while enabling it forwards the
original deltas through the existing natural-scroll transform and OS backend.
No event tag or existing wire layout changes; older peers ignore the new bit.
The IPC preference defaults to true when absent; explicit false remains saved.
Toggle changes use a release barrier and targeted reconnect, without
affecting other devices. Both peers must be updated for this preference to work.
Resource violations close the session. Negotiated input/queue-age stalls enter
recovery; older KCP peers still close. See [reliable input configuration](README.md#experimental-reliable-input-over-dtls)
for timeout settings, transport behavior and experimental limitations.

## Local input recovery barrier (0.17.12)

`InputEmulation::recovery_barrier(handle)` stops and joins background input,
releases tracked keys, buttons and modifiers, and returns an actual absolute
cursor position with display geometry. Failed releases remain tracked for retry;
post-processing preferences survive cleanup. Windows `SendInput` attempts each
event once and propagates rejection instead of spinning indefinitely. Windows
repeat tasks are owned by individual handles and cancellation is joined.

`EmulationProxy::recovery_barrier` retires the previous session/epoch token
immediately and queues cleanup on an independent bounded management channel.
It runs after any already-started backend call finishes; cancelling the caller's
wait does not cancel that OS operation. Success prepares a new token; the
transport driver confirms installation before opening receive gates, then uses
`consume_epoch` for every input. Retry callers retain the original completion;
duplicate or older epochs are rejected. Legacy `Barrier` still only acknowledges
queue consumption. Legacy input/warp requests cannot bypass a managed token.

Windows incoming connections advertise recovery only after the Windows backend
is available and its actual cursor baseline can be read. `recovery_available()`
performs this read-only probe; denied desktop access leaves capability disabled.
macOS receiving connections advertise recovery only while Quartz can read an
active display layout and a cursor position on that layout. Its recovery
barrier joins the key-repeat task before releasing tracked input and reading
the new baseline. The capability is re-probed as the desktop changes, so a
temporary absence of active displays does not persist for the daemon lifetime.
Other receiving backends return `RecoveryUnsupported` until
their background-task and cursor-baseline guarantees are implemented.
Windows currently reports virtual-screen bounding geometry, not individual
monitor contours. A hung backend keeps the gate closed: the driver enforces its
hard deadline and arranges delayed cleanup without cancelling the serial backend
worker. Windows and Mac outgoing KCP capture participate only when the receiving
backend advertises recovery; neither invokes a local emulation barrier or
injects a dummy cursor baseline into the OS.

## Same-connection recovery protocol (0.17.12)

`mousehop_proto::transport::recovery` defines capability bit 6 and an independent
tag-242/version-1 envelope. Recovery requires both peers' capability and selected
KCP. Existing reliable Frame v2 and Motion v1 encodings are unchanged. The outer
header carries a 128-bit nonzero session identifier, u64 input epoch, and sender
role. It wraps reliable frames (including Progress), UDP motion, or an independent
recovery control. Maximum encoded size is 1141 bytes; controls are 29 or 70 bytes.
The baseline is a finite bounded actual cursor position, nonzero layout generation,
confirmed ownership digest, and owner role. The driver must generate a fresh
session identifier for each authenticated DTLS association and verify ownership
and layout against its application state; a digest is not proof by itself.

`transport::recovery::Recovery` is a deterministic, local-monotonic-ms state
machine. The dialer coordinates one `e -> e+1` round; either side may trigger it.
`step(now, Input)` produces an immutable outcome with at most one outbound control,
a barrier or install token, a peer baseline, and a close reason. Request/Prepare/
Prepared/Commit/CommitAck/Activate/ActivateAck bypass the blocked input channel.
The driver must send these via its existing single DTLS writer. It coalesces
repeated barrier/install tokens and prioritizes control over frozen data.

`Barrier` completion means old injection is isolated and keys/background work
are released, not merely that a request was queued. `Installed` completion means
the new KCP, cumulative-motion zero point, receipts and receive consumer are ready.
Only then does the receive gate open. The acceptor sends after Activate and the
dialer sends after ActivateAck, so both receive gates precede the opposite send
gate. `send_epoch(now)` / `receive_epoch(now)` and `accepts_data(now, envelope)`
must be checked immediately before use and again after awaits. Controls enter
`step(Control)` separately. Unknown-result old actions are never a replay source.

The recovery deadline is 2000 ms from first local entry, checked on every event
including backend callbacks, independently of peer liveness. Gate queries also
check expiry even without a tick. Periodic retries are 50 ms; duplicate requests
cannot reset this timer or the hard deadline. One previous terminal response is
cached to repair lost final acknowledgements without another freeze/release.
Only explicitly validated current-session activity may call `PeerActivity`;
old-round controls do not refresh liveness. Close reasons require the driver to
perform safe return and independent/delayed backend cleanup.

The driver generates the 128-bit session with the TLS provider's secure RNG. The
acceptor learns it only from the negotiated first-epoch Offer. Every subsequent
reliable/motion packet validates session, epoch and role, including normal input.
All application input deliveries, including UDP motion, carry a generation guard;
retiring it invalidates receipts and backend tokens immediately. The full session
identity is preserved through EmulationProxy, separately from the KCP conversation.

After CommitAck and local installation, the dialer sends a new-epoch KCP bootstrap
probe before Activate. Only the probe's KCP acknowledgement completes this step;
heartbeat or Progress alone cannot do so. The acceptor's prepared receive gate
permits transport acknowledgements but its input sender remains frozen. Business
input cannot bypass the activation gates. This prevents healthy control traffic
from masking a permanently broken reliable input path.

Capture continuously drains physical events while frozen, suppresses held keys,
buttons, repeats and modifier snapshots until release, and discards paused motion
and scrolling. A bounded drain precedes activation. Actual Windows cursor and
layout fingerprints rebase the sender's model, including negative monitor origins;
no local or remote cursor warp is performed by recovery. Confirmed handover serial
and owner form the ownership digest; unconfirmed transitions fail safely.

The default policy is 600/2000/3000 ms: input stall / fixed recovery / peer silence.
Only stall and peer values are configurable. Legacy I/O and close cleanup retain
their separate 300 ms constants. Negotiated recovery bypasses Legacy watchdogs
and the receiver's independent 1-second ownership timeout. Actual DTLS I/O errors,
permanent writes, integrity violations, resource exhaustion and unsafe backends
still fail closed. Tests do not establish two-machine or field acceptance.

## Held Input Cleanup

The receiving endpoint keeps a per-handle set of forwarded keys and mouse
buttons. Repeated key-downs do not create extra entries; there is no ten-key
limit that could discard a still-held key. Normal return, forced release and
disconnect all retire the receiving handle through the same cleanup path.

Cleanup stops background key repeat first, publishes a zero remote modifier
snapshot, then releases recorded keys and buttons. The macOS backend also checks
its native injector for remaining input, using its existing local-modifier
preservation policy rather than resetting all system keys. Only successful
releases remove records. A failed destroy retains the handle in a pending
cleanup set; `InputEmulation::destroy` now returns a `Result` and
`InputEmulation::retry_cleanup` retries these retired handles.

The serial emulation worker retries before subsequent requests and on its
two-second display poll when idle. While cleanup is failing, it rejects new
input, recovery requests and entry warps instead of injecting over stale state
or destroying/recreating the backend. Input during this interval is discarded,
not replayed later. Final process termination makes a last cleanup attempt and
logs failures; it cannot guarantee release if the OS continues rejecting input.

Sender key-ups remain best-effort and scoped to the old connection/handover;
receiver teardown is authoritative. No old key-up is replayed into a new
session. This improves cleanup reliability but does not establish that stale
macOS combined-session flags are the cause of a particular live reentry issue.

## Problems

The general Idea is to have a bidirectional connection by default, meaning
any connected device can not only receive events but also send events back.

This way when connecting e.g. a PC to a Laptop, either device can be used
to control the other.

It needs to be ensured, that whenever a device is controlled the controlled
device does not transmit the events back to the original sender.
Otherwise events are multiplied and either one of the instances crashes.

To keep the implementation of input backends simple this needs to be handled
on the server level.

## Device State - Active and Inactive
To solve this problem, each device can be in exactly two states:

Either events are sent or received.

This ensures that
- a) Events can never result in a feedback loop.
- b) As soon as a virtual input enters another client, mousehop will stop receiving events,
which ensures clients can only be controlled directly and not indirectly through other clients.

