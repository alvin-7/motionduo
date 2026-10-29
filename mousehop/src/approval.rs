//! A deliberately separate admission protocol. None of these channels are input sessions.
use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    rc::Rc,
    time::{Duration, Instant},
};
use tokio::sync::{Notify, Semaphore};
use webrtc_util::Conn;

pub(crate) const PREFIX: &[u8; 8] = b"MHAP0001";
pub(crate) const TIMEOUT: Duration = Duration::from_secs(60);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Decision {
    Pending,
    Denied,
    Expired,
    Approved,
}
impl Decision {
    fn byte(self) -> u8 {
        match self {
            Self::Pending => 0,
            Self::Denied => 1,
            Self::Expired => 2,
            Self::Approved => 3,
        }
    }
    pub(crate) fn decode(byte: u8) -> Option<Self> {
        Some(match byte {
            0 => Self::Pending,
            1 => Self::Denied,
            2 => Self::Expired,
            3 => Self::Approved,
            _ => return None,
        })
    }
}
struct Request {
    nonce: u64,
    since: Instant,
    decision: Decision,
    attempts: VecDeque<Instant>,
}
#[derive(Clone)]
pub(crate) struct Admissions {
    requests: Rc<RefCell<HashMap<String, Request>>>,
    attempts: Rc<RefCell<VecDeque<Instant>>>,
    pub(crate) permits: std::sync::Arc<Semaphore>,
    changed: Rc<Notify>,
}
impl Default for Admissions {
    fn default() -> Self {
        Self {
            requests: Default::default(),
            attempts: Default::default(),
            permits: std::sync::Arc::new(Semaphore::new(32)),
            changed: Default::default(),
        }
    }
}
impl Admissions {
    fn begin(&self, fingerprint: &str, nonce: u64, now: Instant) -> Decision {
        let mut requests = self.requests.borrow_mut();
        requests.retain(|_, r| now.duration_since(r.since) < Duration::from_secs(120));
        if let Some(r) = requests.get_mut(fingerprint) {
            while r
                .attempts
                .front()
                .is_some_and(|t| now.duration_since(*t) >= TIMEOUT)
            {
                r.attempts.pop_front();
            }
            if r.nonce == nonce {
                return if r.decision == Decision::Pending && now.duration_since(r.since) >= TIMEOUT
                {
                    Decision::Expired
                } else {
                    r.decision
                };
            }
            if r.decision == Decision::Pending || r.attempts.len() >= 8 {
                return Decision::Denied;
            }
        }
        let mut attempts = self.attempts.borrow_mut();
        while attempts
            .front()
            .is_some_and(|t| now.duration_since(*t) >= TIMEOUT)
        {
            attempts.pop_front();
        }
        if nonce == 0 || requests.len() >= 32 || attempts.len() >= 64 {
            return Decision::Denied;
        }
        attempts.push_back(now);
        let mut identity_attempts = requests
            .get(fingerprint)
            .map(|r| r.attempts.clone())
            .unwrap_or_default();
        identity_attempts.push_back(now);
        requests.insert(
            fingerprint.to_owned(),
            Request {
                nonce,
                since: now,
                decision: Decision::Pending,
                attempts: identity_attempts,
            },
        );
        Decision::Pending
    }
    pub(crate) fn pending(&self) -> Vec<(String, u64)> {
        let mut requests = self.requests.borrow_mut();
        for r in requests.values_mut() {
            if r.decision == Decision::Pending && r.since.elapsed() >= TIMEOUT {
                r.decision = Decision::Expired;
            }
        }
        requests
            .iter()
            .filter(|(_, r)| r.decision == Decision::Pending)
            .map(|(fp, r)| (fp.clone(), r.nonce))
            .collect()
    }
    pub(crate) fn is_pending(&self, fp: &str, nonce: u64) -> bool {
        self.requests.borrow().get(fp).is_some_and(|r| {
            r.nonce == nonce && r.decision == Decision::Pending && r.since.elapsed() < TIMEOUT
        })
    }
    pub(crate) fn respond(&self, fp: &str, nonce: u64, decision: Decision) -> bool {
        let mut requests = self.requests.borrow_mut();
        let Some(r) = requests.get_mut(fp) else {
            return false;
        };
        if r.nonce != nonce || r.decision != Decision::Pending || r.since.elapsed() >= TIMEOUT {
            return false;
        }
        r.decision = decision;
        self.changed.notify_waiters();
        true
    }
    pub(crate) async fn receive(&self, conn: &dyn Conn, fp: &str, authorized: bool) -> bool {
        let mut bytes = [0u8; 32];
        let n = match tokio::time::timeout(Duration::from_secs(5), conn.recv(&mut bytes)).await {
            Ok(Ok(n)) => n,
            _ => return false,
        };
        if n != 16 || &bytes[..8] != PREFIX {
            return false;
        }
        let nonce = u64::from_be_bytes(bytes[8..16].try_into().unwrap());
        if authorized {
            let _ = conn.send(&frame(nonce, Decision::Approved)).await;
            return true;
        }
        let decision = match self.begin(fp, nonce, Instant::now()) {
            Decision::Approved => Decision::Denied,
            decision => decision,
        };
        if decision != Decision::Pending {
            let _ = conn.send(&frame(nonce, decision)).await;
            return false;
        }
        let _ = conn.send(&frame(nonce, Decision::Pending)).await;
        loop {
            let notified = self.changed.notified();
            let state = self
                .requests
                .borrow()
                .get(fp)
                .map(|r| (r.nonce, r.decision, r.since));
            let decision = match state {
                Some((n, d, since)) if n == nonce => {
                    if since.elapsed() >= TIMEOUT {
                        Decision::Expired
                    } else {
                        d
                    }
                }
                _ => Decision::Denied,
            };
            if decision != Decision::Pending {
                let _ = conn.send(&frame(nonce, decision)).await;
                break;
            }
            // Periodic wake is confined to a bounded, live request, never an idle service loop.
            tokio::select! { _ = notified => {}, _ = tokio::time::sleep(Duration::from_millis(250)) => {} }
        }
        // Even Approved admission is closed. Authorization requires a fresh normal handshake.
        false
    }
}
pub(crate) fn frame(nonce: u64, decision: Decision) -> [u8; 17] {
    let mut frame = [0u8; 17];
    frame[..8].copy_from_slice(PREFIX);
    frame[8..16].copy_from_slice(&nonce.to_be_bytes());
    frame[16] = decision.byte();
    frame
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn unauthorized_admission_never_returns_input_permission() {
        let a = Admissions::default();
        let server = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let client = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        server.connect(client.local_addr().unwrap()).await.unwrap();
        client.connect(server.local_addr().unwrap()).await.unwrap();
        client
            .send(&frame(9, Decision::Pending)[..16])
            .await
            .unwrap();
        let responder = async {
            let mut bytes = [0; 32];
            let n = client.recv(&mut bytes).await.unwrap();
            assert_eq!(&bytes[..n], &frame(9, Decision::Pending));
            assert!(a.respond("peer", 9, Decision::Approved));
            let n = client.recv(&mut bytes).await.unwrap();
            assert_eq!(&bytes[..n], &frame(9, Decision::Approved));
        };
        let (input, ()) = tokio::join!(a.receive(&server, "peer", false), responder);
        assert!(!input, "approval-only channel was promoted to input");
    }
    #[test]
    fn expired_generation_cannot_be_approved() {
        let a = Admissions::default();
        a.begin("peer", 1, Instant::now() - TIMEOUT);
        assert!(a.pending().is_empty());
        assert!(!a.respond("peer", 1, Decision::Approved));
        assert_eq!(a.begin("peer", 2, Instant::now()), Decision::Pending);
    }
    #[test]
    fn rate_limits_expire_without_growing_storage() {
        let a = Admissions::default();
        let now = Instant::now();
        for nonce in 1..=8 {
            assert_eq!(a.begin("peer", nonce, now), Decision::Pending);
            a.requests.borrow_mut().get_mut("peer").unwrap().decision = Decision::Denied;
        }
        assert_eq!(a.begin("peer", 9, now), Decision::Denied);
        assert_eq!(a.begin("peer", 10, now + TIMEOUT), Decision::Pending);
        assert_eq!(a.attempts.borrow().len(), 1);
        assert_eq!(a.requests.borrow().len(), 1);
    }

    #[test]
    fn global_rate_limit_survives_identity_replacement() {
        let a = Admissions::default();
        let now = Instant::now();
        for nonce in 1..=64 {
            let fp = format!("peer-{nonce}");
            assert_eq!(a.begin(&fp, nonce, now), Decision::Pending);
            // Identity churn must not reset the separate global rate budget.
            a.requests.borrow_mut().remove(&fp);
        }
        assert_eq!(a.begin("new-peer", 65, now), Decision::Denied);
        assert_eq!(a.begin("new-peer", 66, now + TIMEOUT), Decision::Pending);
        assert_eq!(a.attempts.borrow().len(), 1);
    }
    #[test]
    fn denial_requires_a_new_nonce_and_stale_response_cannot_approve() {
        let a = Admissions::default();
        let now = Instant::now();
        assert_eq!(a.begin("a", 1, now), Decision::Pending);
        assert!(a.respond("a", 1, Decision::Denied));
        assert_eq!(
            a.begin("a", 1, now + Duration::from_secs(3)),
            Decision::Denied
        );
        assert_eq!(
            a.begin("a", 2, now + Duration::from_secs(3)),
            Decision::Pending
        );
        assert!(!a.respond("a", 1, Decision::Approved));
        assert!(a.respond("a", 2, Decision::Approved));
    }
    #[test]
    fn bounded_pending_duplicate_and_timeout() {
        let a = Admissions::default();
        let now = Instant::now();
        for n in 1..=32 {
            assert_eq!(a.begin(&n.to_string(), n, now), Decision::Pending);
        }
        assert_eq!(a.begin("overflow", 33, now), Decision::Denied);
        assert_eq!(a.begin("1", 99, now), Decision::Denied);
        assert_eq!(a.begin("1", 1, now + TIMEOUT), Decision::Expired);
        assert_eq!(a.pending().len(), 32);
    }
}
