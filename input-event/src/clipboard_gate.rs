//! Nonblocking process-wide macOS pasteboard ownership fence.
//!
//! The native writer, capture worker and final service publication MUST use
//! `shared()` (or clones of one explicitly supplied instance). Never wait for
//! this gate on an input executor. Native clipboard calls belong on workers.
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

#[derive(Clone, Default)]
pub struct ClipboardGate(Arc<Mutex<State>>);

#[derive(Default)]
struct State {
    observed: Option<u64>,
    remote_through: Option<u64>,
    quarantined: bool,
}

pub struct GateGuard<'a>(MutexGuard<'a, State>);

impl ClipboardGate {
    /// All default native adapters share this process-wide instance. Replacing
    /// it after a counter reset could release a previously suppressed payload.
    pub fn shared() -> Self {
        static GATE: OnceLock<ClipboardGate> = OnceLock::new();
        GATE.get_or_init(Self::default).clone()
    }

    pub fn try_lock(&self) -> Option<GateGuard<'_>> {
        self.0.try_lock().ok().map(GateGuard)
    }

    /// Final publication seam. Only perform a short, nonblocking enqueue in
    /// `publish`; never await, call native APIs, or acquire lifecycle locks here.
    /// False means discard, including contention/poisoning. The generation must
    /// come from the native capture worker, not an arbitrary caller counter.
    pub fn try_publish(&self, generation: u64, publish: impl FnOnce()) -> bool {
        let Some(guard) = self.try_lock() else {
            return false;
        };
        if !guard.allows(generation) {
            return false;
        }
        publish();
        true
    }
}

impl GateGuard<'_> {
    /// Call for a current native observation, not a delayed sample. A reset,
    /// wrap or invalid count permanently quarantines this instance: availability
    /// cannot safely be recovered merely by clearing the high-water mark.
    pub fn observe(&mut self, generation: u64) -> bool {
        if generation == 0
            || generation > i64::MAX as u64
            || self.0.observed.is_some_and(|old| generation < old)
        {
            self.0.quarantined = true;
        }
        self.0.observed = Some(
            self.0
                .observed
                .map_or(generation, |old| old.max(generation)),
        );
        !self.0.quarantined
    }

    /// Record immediately after taking ownership, BEFORE setting any payload.
    /// Never roll this back on verification/registration failure or cancellation.
    pub fn record_remote(&mut self, generation: u64) -> bool {
        let valid = self.observe(generation);
        self.0.remote_through = Some(
            self.0
                .remote_through
                .map_or(generation, |old| old.max(generation)),
        );
        valid
    }

    /// Delayed samples do not update observations: an old sample is a normal
    /// publication race, not evidence that the OS counter reset.
    pub fn allows(&self, generation: u64) -> bool {
        !self.0.quarantined
            && generation != 0
            && generation <= i64::MAX as u64
            && self
                .0
                .remote_through
                .is_none_or(|remote| generation > remote)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_write_remains_fenced_and_next_local_copy_works() {
        let gate = ClipboardGate::default();
        {
            let mut write = gate.try_lock().unwrap();
            assert!(write.record_remote(41));
        }
        assert!(!gate.try_publish(41, || panic!("failed write escaped")));
        assert!(gate.try_publish(42, || {}));
    }
    #[test]
    fn writer_contention_and_delayed_publication_fail_closed() {
        let gate = ClipboardGate::default();
        let mut write = gate.try_lock().unwrap();
        assert!(
            !gate
                .clone()
                .try_publish(40, || panic!("writer in progress"))
        );
        assert!(write.record_remote(41));
        drop(write);
        assert!(!gate.try_publish(40, || panic!("old sample escaped")));
        assert!(!gate.try_publish(41, || panic!("remote sample escaped")));
    }
    #[test]
    fn rollback_wrap_and_reset_never_reopen_remote_payload() {
        for next in [0, 1, u64::MAX, i64::MAX as u64 + 1] {
            let gate = ClipboardGate::default();
            let mut guard = gate.try_lock().unwrap();
            assert!(guard.record_remote(41));
            assert!(!guard.observe(next));
            assert!(!guard.allows(100));
        }
    }
    #[test]
    fn stale_sample_does_not_quarantine_new_local_copy() {
        let gate = ClipboardGate::default();
        {
            let mut guard = gate.try_lock().unwrap();
            assert!(guard.observe(50));
        }
        assert!(gate.try_publish(49, || {}));
        assert!(gate.try_publish(51, || {}));
    }
    #[test]
    fn poisoned_gate_never_publishes() {
        let gate = ClipboardGate::default();
        let other = gate.clone();
        let _ = std::thread::spawn(move || {
            let _guard = other.try_lock().unwrap();
            panic!("worker failure");
        })
        .join();
        assert!(!gate.try_publish(1, || panic!("poison escaped")));
    }
}
