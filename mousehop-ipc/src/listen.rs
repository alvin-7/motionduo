use futures::{Stream, StreamExt, stream::SelectAll};
#[cfg(unix)]
use std::path::PathBuf;
use std::{
    io::ErrorKind,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use tokio::io::{AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_stream::wrappers::LinesStream;

#[cfg(unix)]
use tokio::net::UnixListener;
#[cfg(unix)]
use tokio::net::UnixStream;
#[cfg(unix)]
use tokio::net::unix::OwnedReadHalf;

#[cfg(windows)]
use tokio::net::TcpListener;
#[cfg(windows)]
use tokio::net::tcp::OwnedReadHalf;

use crate::{FrontendEvent, FrontendRequest, IpcError, IpcListenerCreationError};

pub struct AsyncFrontendListener {
    #[cfg(windows)]
    listener: TcpListener,
    #[cfg(unix)]
    listener: UnixListener,
    #[cfg(unix)]
    socket_path: PathBuf,
    line_streams: SelectAll<LinesStream<BufReader<OwnedReadHalf>>>,
    tx_streams: Vec<FrontendWriter>,
}

struct FrontendWriter {
    sender: mpsc::Sender<Arc<[u8]>>,
    task: JoinHandle<()>,
}

impl FrontendWriter {
    fn new(mut writer: impl AsyncWrite + Unpin + Send + 'static) -> Self {
        let (sender, mut receiver) = mpsc::channel::<Arc<[u8]>>(64);
        let task = tokio::spawn(async move {
            while let Some(frame) = receiver.recv().await {
                if !matches!(
                    tokio::time::timeout(Duration::from_secs(2), writer.write_all(&frame)).await,
                    Ok(Ok(()))
                ) {
                    break;
                }
            }
        });
        Self { sender, task }
    }
}

impl Drop for FrontendWriter {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl AsyncFrontendListener {
    pub async fn new() -> Result<Self, IpcListenerCreationError> {
        #[cfg(unix)]
        let (socket_path, listener) = {
            let socket_path = crate::default_socket_path()?;

            log::debug!("remove socket: {socket_path:?}");
            if socket_path.exists() {
                // try to connect to see if some other instance
                // of mousehop is already running
                match UnixStream::connect(&socket_path).await {
                    // connected -> mousehop is already running
                    Ok(_) => return Err(IpcListenerCreationError::AlreadyRunning),
                    // mousehop is not running but a socket was left behind
                    Err(e) => {
                        log::debug!("{socket_path:?}: {e} - removing left behind socket");
                        let _ = std::fs::remove_file(&socket_path);
                    }
                }
            }
            let listener = match UnixListener::bind(&socket_path) {
                Ok(ls) => ls,
                // some other mousehop instance has bound the socket in the meantime
                Err(e) if e.kind() == ErrorKind::AddrInUse => {
                    return Err(IpcListenerCreationError::AlreadyRunning);
                }
                Err(e) => return Err(IpcListenerCreationError::Bind(e)),
            };
            (socket_path, listener)
        };

        #[cfg(windows)]
        let listener =
            match TcpListener::bind(crate::tcp_endpoint().map_err(IpcListenerCreationError::Bind)?)
                .await
            {
                Ok(ls) => ls,
                // some other mousehop instance has bound the socket in the meantime
                Err(e) if e.kind() == ErrorKind::AddrInUse => {
                    return Err(IpcListenerCreationError::AlreadyRunning);
                }
                Err(e) => return Err(IpcListenerCreationError::Bind(e)),
            };

        let adapter = Self {
            listener,
            #[cfg(unix)]
            socket_path,
            line_streams: SelectAll::new(),
            tx_streams: vec![],
        };

        Ok(adapter)
    }

    pub async fn broadcast(&mut self, notify: FrontendEvent) {
        // encode event
        let mut json = serde_json::to_string(&notify).unwrap();
        json.push('\n');

        let frame: Arc<[u8]> = json.into_bytes().into();
        // A slow or hidden frontend must never park input dispatch. Disconnect
        // it on overflow; the reconnecting frontend obtains a fresh Sync.
        self.tx_streams
            .retain(|tx| tx.sender.try_send(frame.clone()).is_ok());
    }
}

#[cfg(unix)]
impl Drop for AsyncFrontendListener {
    fn drop(&mut self) {
        log::debug!("remove socket: {:?}", self.socket_path);
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

impl Stream for AsyncFrontendListener {
    type Item = Result<FrontendRequest, IpcError>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Poll::Ready(Some(Ok(l))) = self.line_streams.poll_next_unpin(cx) {
            let request = serde_json::from_str(l.as_str()).map_err(|e| e.into());
            return Poll::Ready(Some(request));
        }
        let mut sync = false;
        while let Poll::Ready(Ok((stream, _))) = self.listener.poll_accept(cx) {
            if self.tx_streams.len() >= 16 {
                continue;
            }
            let (rx, tx) = stream.into_split();
            let buf_reader = BufReader::new(rx);
            let lines = buf_reader.lines();
            let lines = LinesStream::new(lines);
            self.line_streams.push(lines);
            self.tx_streams.push(FrontendWriter::new(tx));
            sync = true;
        }
        if sync {
            Poll::Ready(Some(Ok(FrontendRequest::Sync)))
        } else {
            Poll::Pending
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;

    #[tokio::test]
    async fn writer_finishes_short_writes_and_preserves_frames() {
        let (mut reader, writer) = tokio::io::duplex(3);
        let writer = FrontendWriter::new(writer);
        let expected = b"{\"DiscoveryEmail\":null}\n{\"PairingRequests\":[]}\n";
        writer.sender.try_send(Arc::from(&expected[..24])).unwrap();
        writer.sender.try_send(Arc::from(&expected[24..])).unwrap();
        let mut received = vec![0; expected.len()];
        tokio::time::timeout(Duration::from_secs(1), reader.read_exact(&mut received))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(received, expected);
    }

    #[tokio::test]
    async fn slow_frontend_has_a_bounded_nonblocking_queue() {
        let (_reader, writer) = tokio::io::duplex(1);
        let writer = FrontendWriter::new(writer);
        let frame: Arc<[u8]> = Arc::from(&b"frame\n"[..]);
        for _ in 0..64 {
            writer.sender.try_send(frame.clone()).unwrap();
        }
        assert!(matches!(
            writer.sender.try_send(frame),
            Err(mpsc::error::TrySendError::Full(_))
        ));
    }
}
