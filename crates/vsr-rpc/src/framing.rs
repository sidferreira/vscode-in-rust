//! upstream: src/vs/base/parts/ipc/common/ipc.net.ts @ 50e1350fc78d
//!
//! The framing of `PersistentProtocol` (`ipc.net.ts`): every message on the socket is a 13-byte
//! header followed by `DATA_LENGTH` bytes.
//!
//! ```text
//! | TYPE (u8) | ID (u32be) | ACK (u32be) | DATA_LENGTH (u32be) | DATA |
//! ```
//!
//! Only `Regular` frames are numbered (`ID`, from 1) and acknowledged (`ACK` is the id of the last
//! regular frame the sender has received); every other type carries id 0. The extension host talks
//! over a plain socket here (`VSCODE_EXTHOST_IPC_HOOK`), so there are no WebSocket frames.
//!
//! [`FrameReader`] cuts a byte stream into frames; [`Protocol`] is the state machine (ids, acks,
//! replay) and does no I/O: it returns the frames to write.

use std::collections::VecDeque;

/// `ProtocolConstants.HeaderLength`.
pub const HEADER_LENGTH: usize = 13;
/// `ProtocolConstants.AcknowledgeTime`: an ack is due at most this long after a regular frame.
pub const ACKNOWLEDGE_TIME_MS: u64 = 2000;

/// `ProtocolMessageType` (value 4 is unused upstream).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    None = 0,
    Regular = 1,
    Control = 2,
    Ack = 3,
    Disconnect = 5,
    ReplayRequest = 6,
    Pause = 7,
    Resume = 8,
    KeepAlive = 9,
}

impl FrameType {
    pub fn from_u8(v: u8) -> Option<FrameType> {
        Some(match v {
            0 => FrameType::None,
            1 => FrameType::Regular,
            2 => FrameType::Control,
            3 => FrameType::Ack,
            5 => FrameType::Disconnect,
            6 => FrameType::ReplayRequest,
            7 => FrameType::Pause,
            8 => FrameType::Resume,
            9 => FrameType::KeepAlive,
            _ => return None,
        })
    }
}

/// One frame. `ty` is the raw type byte: an unknown type is kept and ignored, as upstream does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub ty: u8,
    pub id: u32,
    pub ack: u32,
    pub data: Vec<u8>,
}

impl Frame {
    pub fn new(ty: FrameType, id: u32, ack: u32, data: Vec<u8>) -> Frame {
        Frame {
            ty: ty as u8,
            id,
            ack,
            data,
        }
    }

    pub fn frame_type(&self) -> Option<FrameType> {
        FrameType::from_u8(self.ty)
    }

    /// Header plus data, as `ProtocolWriter.write` lays it out.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER_LENGTH + self.data.len());
        out.push(self.ty);
        out.extend_from_slice(&self.id.to_be_bytes());
        out.extend_from_slice(&self.ack.to_be_bytes());
        out.extend_from_slice(&(self.data.len() as u32).to_be_bytes());
        out.extend_from_slice(&self.data);
        out
    }
}

/// Cuts a byte stream into frames (`ProtocolReader`): feed it whatever the socket returned.
#[derive(Default)]
pub struct FrameReader {
    buf: Vec<u8>,
}

impl FrameReader {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add bytes and return every frame that is now complete. A partial header or body stays
    /// buffered for the next call.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<Frame> {
        self.buf.extend_from_slice(bytes);
        let mut frames = Vec::new();
        let mut pos = 0;
        while self.buf.len() - pos >= HEADER_LENGTH {
            let h = &self.buf[pos..pos + HEADER_LENGTH];
            let len = u32::from_be_bytes([h[9], h[10], h[11], h[12]]) as usize;
            if self.buf.len() - pos < HEADER_LENGTH + len {
                break;
            }
            frames.push(Frame {
                ty: h[0],
                id: u32::from_be_bytes([h[1], h[2], h[3], h[4]]),
                ack: u32::from_be_bytes([h[5], h[6], h[7], h[8]]),
                data: self.buf[pos + HEADER_LENGTH..pos + HEADER_LENGTH + len].to_vec(),
            });
            pos += HEADER_LENGTH + len;
        }
        self.buf.drain(..pos);
        frames
    }

    /// Bytes received but not yet part of a complete frame.
    pub fn buffered(&self) -> usize {
        self.buf.len()
    }
}

/// What [`Protocol::receive`] produced for one frame.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Received {
    /// A regular message to hand to the layer above (in order, exactly once).
    pub message: Option<Vec<u8>>,
    /// A control message (`sendControl` upstream; unused after the handshake).
    pub control: Option<Vec<u8>>,
    /// Frames to write back (a replay request or the answer to one).
    pub to_send: Vec<Frame>,
    /// The peer sent `Disconnect`.
    pub disconnected: bool,
}

/// Ids, acks and the unacknowledged queue of `PersistentProtocol`, without sockets or timers.
#[derive(Default)]
pub struct Protocol {
    outgoing_msg_id: u32,
    incoming_msg_id: u32,
    /// The last `ack` value we sent.
    incoming_ack_id: u32,
    /// The last `ack` value the peer sent.
    outgoing_ack_id: u32,
    unacked: VecDeque<Frame>,
    peer_paused: bool,
}

impl Protocol {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number a message, piggyback our ack on it, keep it until the peer acknowledges it, and
    /// return the frame to write (`PersistentProtocol.send`).
    pub fn send(&mut self, data: Vec<u8>) -> Frame {
        self.outgoing_msg_id += 1;
        self.incoming_ack_id = self.incoming_msg_id;
        let frame = Frame::new(
            FrameType::Regular,
            self.outgoing_msg_id,
            self.incoming_ack_id,
            data,
        );
        self.unacked.push_back(frame.clone());
        frame
    }

    /// An unnumbered `Control` frame (`sendControl`).
    pub fn send_control(&self, data: Vec<u8>) -> Frame {
        Frame::new(FrameType::Control, 0, 0, data)
    }

    /// `sendResume`: the extension host sends one first thing; we do the same.
    pub fn resume_frame(&self) -> Frame {
        Frame::new(FrameType::Resume, 0, 0, Vec::new())
    }

    pub fn disconnect_frame(&self) -> Frame {
        Frame::new(FrameType::Disconnect, 0, 0, Vec::new())
    }

    /// True when regular frames arrived that we have not acknowledged yet. Upstream sends an
    /// explicit `Ack` within [`ACKNOWLEDGE_TIME_MS`] when no outgoing message piggybacks it.
    pub fn ack_due(&self) -> bool {
        self.incoming_msg_id > self.incoming_ack_id
    }

    /// The explicit `Ack` frame (`_sendAck`).
    pub fn make_ack(&mut self) -> Frame {
        self.incoming_ack_id = self.incoming_msg_id;
        Frame::new(FrameType::Ack, 0, self.incoming_ack_id, Vec::new())
    }

    /// Whether the peer asked us to stop writing (`Pause`) and has not resumed.
    pub fn peer_paused(&self) -> bool {
        self.peer_paused
    }

    pub fn unacknowledged(&self) -> usize {
        self.unacked.len()
    }

    /// `_receiveMessage`: drop what the peer acknowledged, then act on the type.
    pub fn receive(&mut self, frame: Frame) -> Received {
        let mut out = Received::default();
        if frame.ack > self.outgoing_ack_id {
            self.outgoing_ack_id = frame.ack;
            while self.unacked.front().is_some_and(|f| f.id <= frame.ack) {
                self.unacked.pop_front();
            }
        }
        match frame.frame_type() {
            Some(FrameType::Regular) => {
                if frame.id > self.incoming_msg_id {
                    if frame.id != self.incoming_msg_id + 1 {
                        // A gap: ask the peer to send everything again.
                        out.to_send
                            .push(Frame::new(FrameType::ReplayRequest, 0, 0, Vec::new()));
                    } else {
                        self.incoming_msg_id = frame.id;
                        out.message = Some(frame.data);
                    }
                }
            }
            Some(FrameType::Control) => out.control = Some(frame.data),
            Some(FrameType::Disconnect) => out.disconnected = true,
            Some(FrameType::ReplayRequest) => out.to_send.extend(self.unacked.iter().cloned()),
            Some(FrameType::Pause) => self.peer_paused = true,
            Some(FrameType::Resume) => self.peer_paused = false,
            // None, Ack (handled above), KeepAlive and unknown types: nothing to do.
            _ => {}
        }
        out
    }
}
