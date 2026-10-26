//! Never wait for daemon IPC on GTK's main thread.

use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{SyncSender, TrySendError, sync_channel},
    },
    time::Duration,
};

use mousehop_ipc::{FrontendRequest, FrontendRequestWriter};

const QUEUE_CAPACITY: usize = 64;

pub(crate) struct FrontendWriter {
    sender: SyncSender<FrontendRequest>,
    active: Arc<AtomicBool>,
}

impl FrontendWriter {
    pub(crate) fn new(mut writer: FrontendRequestWriter) -> io::Result<Self> {
        writer.set_write_timeout(Some(Duration::from_secs(1)))?;
        Self::spawn(move |request| writer.request(request))
    }

    fn spawn(
        mut write: impl FnMut(FrontendRequest) -> io::Result<()> + Send + 'static,
    ) -> io::Result<Self> {
        let (sender, receiver) = sync_channel(QUEUE_CAPACITY);
        let active = Arc::new(AtomicBool::new(true));
        let worker_active = active.clone();
        std::thread::Builder::new()
            .name("frontend-ipc-writer".into())
            .spawn(move || {
                while let Ok(request) = receiver.recv() {
                    if !worker_active.load(Ordering::Acquire) {
                        break;
                    }
                    if let Err(error) = write(request) {
                        log::warn!("daemon IPC write failed: {error}");
                        break;
                    }
                }
            })?;
        Ok(Self { sender, active })
    }

    pub(crate) fn request(&self, request: FrontendRequest) -> io::Result<()> {
        self.sender.try_send(request).map_err(|error| match error {
            TrySendError::Full(_) => io::Error::new(
                io::ErrorKind::WouldBlock,
                "daemon request queue full; retry the change",
            ),
            TrySendError::Disconnected(_) => {
                io::Error::new(io::ErrorKind::BrokenPipe, "daemon IPC writer disconnected")
            }
        })
    }
}

impl Drop for FrontendWriter {
    fn drop(&mut self) {
        // Reconnect replaces the writer: discard old queued commands instead
        // of applying them after the new connection has already synchronized.
        self.active.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    #[test]
    fn blocked_daemon_keeps_producer_nonblocking_and_queue_bounded() {
        let (started_tx, started_rx) = channel();
        let (release_tx, release_rx) = channel();
        let writer = FrontendWriter::spawn(move |_| {
            started_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            Ok(())
        })
        .unwrap();
        writer.request(FrontendRequest::Sync).unwrap();
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        for _ in 0..QUEUE_CAPACITY {
            writer.request(FrontendRequest::Sync).unwrap();
        }
        assert_eq!(
            writer.request(FrontendRequest::Sync).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        drop(writer);
        release_tx.send(()).unwrap();
        // A replaced/dropped writer must not drain stale queued requests.
        assert!(started_rx.recv_timeout(Duration::from_secs(2)).is_err());
    }

    #[test]
    fn requests_keep_fifo_order() {
        let (sent_tx, sent_rx) = channel();
        let writer = FrontendWriter::spawn(move |request| {
            sent_tx.send(format!("{request:?}")).unwrap();
            Ok(())
        })
        .unwrap();
        writer.request(FrontendRequest::Sync).unwrap();
        writer.request(FrontendRequest::Enumerate()).unwrap();
        assert_eq!(
            sent_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            "Sync"
        );
        assert_eq!(
            sent_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            "Enumerate"
        );
    }
}
