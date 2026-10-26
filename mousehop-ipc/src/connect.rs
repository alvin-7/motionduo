use crate::{ConnectionError, FrontendEvent, FrontendRequest, IpcError};
use std::{
    cmp::min,
    io::{self, BufReader, LineWriter, Lines, prelude::*},
    thread,
    time::Duration,
};

#[cfg(unix)]
use std::os::unix::net::UnixStream;
#[cfg(unix)]
use std::path::Path;

#[cfg(windows)]
use std::net::TcpStream;

#[cfg(unix)]
type FrontendStream = UnixStream;
#[cfg(windows)]
type FrontendStream = TcpStream;

pub struct FrontendEventReader {
    #[cfg(unix)]
    lines: Lines<BufReader<UnixStream>>,
    #[cfg(windows)]
    lines: Lines<BufReader<TcpStream>>,
}

pub struct FrontendRequestWriter {
    #[cfg(unix)]
    line_writer: LineWriter<UnixStream>,
    #[cfg(windows)]
    line_writer: LineWriter<TcpStream>,
}

impl FrontendEventReader {
    pub fn next_event(&mut self) -> Option<Result<FrontendEvent, IpcError>> {
        match self.lines.next()? {
            Err(e) => Some(Err(e.into())),
            Ok(l) => Some(serde_json::from_str(l.as_str()).map_err(|e| e.into())),
        }
    }
}

impl FrontendRequestWriter {
    /// Bound blocking writes for callers using a dedicated writer thread.
    pub fn set_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.line_writer.get_ref().set_write_timeout(timeout)
    }

    pub fn request(&mut self, request: FrontendRequest) -> Result<(), io::Error> {
        let mut json = serde_json::to_string(&request).unwrap();
        log::debug!("requesting: {json}");
        json.push('\n');
        if let Err(error) = self.line_writer.write_all(json.as_bytes()) {
            // A partial JSON frame cannot be retried on the same stream.
            // Wake the reader too, so frontends can reconnect and resync.
            let _ = self
                .line_writer
                .get_ref()
                .shutdown(std::net::Shutdown::Both);
            return Err(error);
        }
        Ok(())
    }
}

pub fn connect() -> Result<(FrontendEventReader, FrontendRequestWriter), ConnectionError> {
    frontend_connection(wait_for_service()?)
}

/// Attempt one connection to the frontend endpoint.
///
/// Unlike [`connect`], this returns the first connection error instead of
/// retrying until the service appears. Callers that drive their own retry
/// loop can therefore keep that loop bounded and off latency-sensitive
/// threads.
pub fn connect_once() -> Result<(FrontendEventReader, FrontendRequestWriter), ConnectionError> {
    connect_once_to_service()
}

#[cfg(unix)]
fn connect_once_to_service() -> Result<(FrontendEventReader, FrontendRequestWriter), ConnectionError>
{
    let socket_path = crate::default_socket_path()?;
    connect_once_to_path(&socket_path)
}

#[cfg(unix)]
fn connect_once_to_path(
    socket_path: &Path,
) -> Result<(FrontendEventReader, FrontendRequestWriter), ConnectionError> {
    frontend_connection(connect_to_path(socket_path)?)
}

#[cfg(windows)]
fn connect_once_to_service() -> Result<(FrontendEventReader, FrontendRequestWriter), ConnectionError>
{
    frontend_connection(connect_to_service()?)
}

fn frontend_connection(
    rx: FrontendStream,
) -> Result<(FrontendEventReader, FrontendRequestWriter), ConnectionError> {
    let tx = rx.try_clone()?;
    let buf_reader = BufReader::new(rx);
    let lines = buf_reader.lines();
    let line_writer = LineWriter::new(tx);
    let reader = FrontendEventReader { lines };
    let writer = FrontendRequestWriter { line_writer };
    Ok((reader, writer))
}

/// wait for the mousehop socket to come online
#[cfg(unix)]
fn wait_for_service() -> Result<UnixStream, ConnectionError> {
    let socket_path = crate::default_socket_path()?;
    let mut duration = Duration::from_millis(10);
    loop {
        if let Ok(stream) = connect_to_path(&socket_path) {
            break Ok(stream);
        }
        // a signaling mechanism or inotify could be used to
        // improve this
        thread::sleep(exponential_back_off(&mut duration));
    }
}

#[cfg(windows)]
fn wait_for_service() -> Result<TcpStream, ConnectionError> {
    let mut duration = Duration::from_millis(10);
    loop {
        if let Ok(stream) = connect_to_service() {
            break Ok(stream);
        }
        thread::sleep(exponential_back_off(&mut duration));
    }
}

#[cfg(unix)]
fn connect_to_path(socket_path: &Path) -> Result<UnixStream, ConnectionError> {
    Ok(UnixStream::connect(socket_path)?)
}

#[cfg(windows)]
fn connect_to_service() -> Result<TcpStream, ConnectionError> {
    Ok(TcpStream::connect("127.0.0.1:5252")?)
}

fn exponential_back_off(duration: &mut Duration) -> Duration {
    let new = duration.saturating_mul(2);
    *duration = min(new, Duration::from_secs(1));
    *duration
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::io::ErrorKind;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn blocked_write_times_out_and_disconnects_partial_frame() {
        let (client, mut server) = UnixStream::pair().unwrap();
        let mut reader = client.try_clone().unwrap();
        reader
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut writer = FrontendRequestWriter {
            line_writer: LineWriter::new(client),
        };
        writer
            .set_write_timeout(Some(Duration::from_millis(30)))
            .unwrap();
        let error = writer
            .request(FrontendRequest::AuthorizeKey(
                "x".repeat(4 * 1024 * 1024),
                String::new(),
            ))
            .unwrap_err();
        assert!(matches!(
            error.kind(),
            ErrorKind::WouldBlock | ErrorKind::TimedOut
        ));
        assert_eq!(
            reader.read(&mut [0]).unwrap(),
            0,
            "reader must wake for reconnect"
        );
        server
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut partial = Vec::new();
        server.read_to_end(&mut partial).unwrap();
        assert!(
            !partial.ends_with(b"\n"),
            "failed frame must never look complete"
        );
    }

    #[test]
    fn one_shot_connect_returns_when_socket_is_missing() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after Unix epoch")
            .as_nanos();
        let socket_path = std::env::temp_dir().join(format!(
            "mousehop-ipc-missing-{}-{nonce}.sock",
            std::process::id()
        ));
        assert!(!socket_path.exists());

        let error = match connect_once_to_path(&socket_path) {
            Ok(_) => panic!("missing socket must fail"),
            Err(error) => error,
        };
        match error {
            ConnectionError::Io(error) => assert_eq!(error.kind(), ErrorKind::NotFound),
            other => panic!("unexpected connection error: {other}"),
        }
    }
}
