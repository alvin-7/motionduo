//! Socket-free clipboard KCP prototype. No negotiation, authentication, consent,
//! epochs or production routing: callers must provide those before integration.
use kcp::Kcp;
use std::{
    cell::RefCell,
    collections::VecDeque,
    io::{self, Write},
    rc::Rc,
};

// A 64 KiB chunk plus bounded framing, unlike the input session's ~1 KiB cap.
// Kept local deliberately: this prototype does not define a peer wire protocol.
const MAX_MESSAGE: usize = 65_558;
const MTU: usize = 1200;
const HEADER: usize = 24;
const MSS: usize = MTU - HEADER;
const MAX_FRAGMENTS: usize = MAX_MESSAGE.div_ceil(MSS);
const SEND_SEGMENTS: usize = 128;
// kcp 0.6 clamps its receive window to at least 128 segments. Its receive queue
// and out-of-order buffer can each retain one window (not just one message).
const RECEIVE_WINDOW: u16 = 128;
const RECEIVE_MESSAGES: usize = 2;
const OUTPUT_PACKETS: usize = 260;
type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum Error {
    #[error("clipboard message must be in 1..={MAX_MESSAGE} bytes")]
    MessageSize,
    #[error("clipboard send queue is full; retry after feedback")]
    Backpressure,
    #[error("invalid clipboard KCP datagram")]
    Datagram,
    #[error("clipboard KCP failed")]
    Failed,
    #[error("clipboard session is closed")]
    Closed,
}

#[derive(Clone, Default)]
struct Output(Rc<RefCell<VecDeque<Vec<u8>>>>);
impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut queue = self.0.borrow_mut();
        if bytes.len() > MTU || queue.len() >= OUTPUT_PACKETS {
            return Err(io::Error::other("clipboard KCP output budget"));
        }
        queue.push_back(bytes.to_vec());
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// One full-duplex session, independent of input KCP state. Advance with a local
/// monotonic millisecond clock and drain datagrams into a separately bounded I/O
/// queue. Transport ACKs are NOT application-consumption or clipboard-write ACKs.
/// Payload storage is bounded by 128 send segments, two 128-segment receive
/// windows, two MAX_MESSAGE deliveries and 260 MTU outputs (< 1 MiB total);
/// allocator/KCP bookkeeping and caller-owned I/O buffers are additional.
pub(crate) struct Session {
    kcp: Option<Kcp<Output>>,
    output: Output,
    incoming: VecDeque<Vec<u8>>,
    conv: u32,
}

impl Session {
    pub(crate) fn new(conv: u32, now: u32) -> Result<Self> {
        let output = Output::default();
        let mut kcp = Kcp::new(conv, output.clone());
        kcp.set_mtu(MTU).map_err(|_| Error::Failed)?;
        kcp.set_wndsize(SEND_SEGMENTS as u16, RECEIVE_WINDOW);
        kcp.set_nodelay(true, 10, 2, false);
        kcp.update(now).map_err(|_| Error::Failed)?;
        Ok(Self {
            kcp: Some(kcp),
            output,
            incoming: VecDeque::new(),
            conv,
        })
    }

    pub(crate) fn send(&mut self, bytes: &[u8]) -> Result<()> {
        let kcp = self.kcp.as_mut().ok_or(Error::Closed)?;
        if bytes.is_empty() || bytes.len() > MAX_MESSAGE {
            return Err(Error::MessageSize);
        }
        // wait_snd counts fragments, not messages. Reserve the entire message
        // before calling send, so rejection cannot leave a partial message.
        if kcp.wait_snd() + bytes.len().div_ceil(MSS) > SEND_SEGMENTS {
            return Err(Error::Backpressure);
        }
        if kcp.send(bytes).is_err() {
            return self.fail(Error::Failed);
        }
        Ok(())
    }

    pub(crate) fn input(&mut self, bytes: &[u8]) -> Result<()> {
        if self.kcp.is_none() {
            return Err(Error::Closed);
        }
        if !valid_datagram(bytes, self.conv) {
            return self.fail(Error::Datagram);
        }
        let kcp = self.kcp.as_mut().unwrap();
        // Flush ACKs on every bounded datagram: duplicate input must not grow
        // kcp's otherwise unbounded ACK list between timer ticks.
        if kcp.input(bytes).is_err() || kcp.flush_ack().is_err() {
            return self.fail(Error::Failed);
        }
        self.collect()
    }

    pub(crate) fn tick(&mut self, now: u32) -> Result<()> {
        let kcp = self.kcp.as_mut().ok_or(Error::Closed)?;
        if kcp.update(now).is_err() || kcp.is_dead_link() {
            return self.fail(Error::Failed);
        }
        self.collect()
    }

    fn collect(&mut self) -> Result<()> {
        let kcp = self.kcp.as_mut().ok_or(Error::Closed)?;
        while self.incoming.len() < RECEIVE_MESSAGES {
            let Ok(size) = kcp.peeksize() else { break };
            if size == 0 || size > MAX_MESSAGE {
                return self.fail(Error::MessageSize);
            }
            let mut bytes = vec![0; size];
            if kcp.recv(&mut bytes).is_err() {
                return self.fail(Error::Failed);
            }
            self.incoming.push_back(bytes);
        }
        Ok(())
    }

    pub(crate) fn receive(&mut self) -> Result<Option<Vec<u8>>> {
        self.collect()?;
        Ok(self.incoming.pop_front())
    }

    pub(crate) fn packet(&mut self) -> Option<Vec<u8>> {
        self.output.0.borrow_mut().pop_front()
    }

    pub(crate) fn close(&mut self) {
        self.kcp = None;
        self.incoming.clear();
        self.output.0.borrow_mut().clear();
    }

    fn fail<T>(&mut self, error: Error) -> Result<T> {
        self.close();
        Err(error)
    }
}

fn valid_datagram(mut bytes: &[u8], conv: u32) -> bool {
    if bytes.is_empty() || bytes.len() > MTU {
        return false;
    }
    while !bytes.is_empty() {
        if bytes.len() < HEADER || u32::from_le_bytes(bytes[..4].try_into().unwrap()) != conv {
            return false;
        }
        let size = u32::from_le_bytes(bytes[20..24].try_into().unwrap()) as usize;
        if size > MSS || bytes.len() < HEADER + size {
            return false;
        }
        match bytes[4] {
            81 if size > 0 && (bytes[5] as usize) < MAX_FRAGMENTS => {}
            82..=84 if size == 0 && bytes[5] == 0 => {}
            _ => return false,
        }
        bytes = &bytes[HEADER + size..];
    }
    true
}

#[cfg(test)]
mod tests;
