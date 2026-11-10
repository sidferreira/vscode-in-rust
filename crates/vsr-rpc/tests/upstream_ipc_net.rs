//! upstream: src/vs/base/parts/ipc/test/node/ipc.net.test.ts @ 50e1350fc78d
//!
//! Ported over two [`Protocol`] state machines joined by an in-memory "ether" that moves encoded
//! frames through a [`FrameReader`]. Upstream's `PersistentProtocol` timers (ack after 2s, 20s
//! timeouts, keep-alive) have no equivalent in the sans-IO state machine: the ack-after-a-while
//! test checks `ack_due` / `make_ack` instead of waiting. Not ported: reconnection, socket
//! timeout, keep-alive, pausing tests and every `WebSocketNodeSocket` test (there are no WebSocket
//! frames on the extension host's socket).

use vsr_rpc::framing::{Frame, FrameReader, FrameType, HEADER_LENGTH, Protocol};

/// One end: a protocol, a reader for what arrives and the messages it delivered.
struct End {
    protocol: Protocol,
    reader: FrameReader,
    messages: Vec<Vec<u8>>,
}

impl End {
    fn new() -> End {
        End {
            protocol: Protocol::new(),
            reader: FrameReader::new(),
            messages: vec![],
        }
    }
    /// Hand over bytes from the wire; returns frames the protocol wants written back.
    fn receive(&mut self, bytes: &[u8]) -> Vec<Frame> {
        let mut back = vec![];
        for frame in self.reader.push(bytes) {
            let got = self.protocol.receive(frame);
            back.extend(got.to_send);
            if let Some(m) = got.message {
                self.messages.push(m);
            }
        }
        back
    }
}

fn deliver(frame: &Frame, to: &mut End) {
    assert!(to.receive(&frame.encode()).is_empty());
}

// upstream: IPC, Socket Protocol > read/write
#[test]
fn read_write() {
    let mut a = End::new();
    let mut b = End::new();
    deliver(&a.protocol.send(b"foobarfarboo".to_vec()), &mut b);
    assert_eq!(b.messages.pop().unwrap(), b"foobarfarboo");
    deliver(&a.protocol.send(vec![123]), &mut b);
    assert_eq!(b.messages.pop().unwrap()[0], 123);
}

// upstream: IPC, Socket Protocol > read/write, object data
#[test]
fn read_write_object_data() {
    let mut a = End::new();
    let mut b = End::new();
    let data = serde_json::json!({ "pi": std::f64::consts::PI, "foo": "bar", "more": true, "data": "Hello World".chars().map(|c| c.to_string()).collect::<Vec<_>>() });
    deliver(&a.protocol.send(serde_json::to_vec(&data).unwrap()), &mut b);
    let got: serde_json::Value = serde_json::from_slice(&b.messages.pop().unwrap()).unwrap();
    assert_eq!(got, data);
}

// upstream: PersistentProtocol reconnection > acks get piggybacked with messages
#[test]
fn acks_get_piggybacked_with_messages() {
    let mut a = End::new();
    let mut b = End::new();
    let f1 = a.protocol.send(b"a1".to_vec());
    assert_eq!(
        (a.protocol.unacknowledged(), b.protocol.unacknowledged()),
        (1, 0)
    );
    let f2 = a.protocol.send(b"a2".to_vec());
    assert_eq!(a.protocol.unacknowledged(), 2);
    let f3 = a.protocol.send(b"a3".to_vec());
    assert_eq!(
        (a.protocol.unacknowledged(), b.protocol.unacknowledged()),
        (3, 0)
    );

    for (f, want) in [(f1, "a1"), (f2, "a2"), (f3, "a3")] {
        deliver(&f, &mut b);
        assert_eq!(b.messages.pop().unwrap(), want.as_bytes());
        // Receiving does not acknowledge anything by itself.
        assert_eq!(
            (a.protocol.unacknowledged(), b.protocol.unacknowledged()),
            (3, 0)
        );
    }

    // b answers: its message carries the ack of a1..a3.
    let fb = b.protocol.send(b"b1".to_vec());
    assert_eq!(
        (a.protocol.unacknowledged(), b.protocol.unacknowledged()),
        (3, 1)
    );
    deliver(&fb, &mut a);
    assert_eq!(a.messages.pop().unwrap(), b"b1");
    assert_eq!(
        (a.protocol.unacknowledged(), b.protocol.unacknowledged()),
        (0, 1)
    );

    let f4 = a.protocol.send(b"a4".to_vec());
    assert_eq!(
        (a.protocol.unacknowledged(), b.protocol.unacknowledged()),
        (1, 1)
    );
    deliver(&f4, &mut b);
    assert_eq!(b.messages.pop().unwrap(), b"a4");
    assert_eq!(
        (a.protocol.unacknowledged(), b.protocol.unacknowledged()),
        (1, 0)
    );
}

// upstream: PersistentProtocol reconnection > ack gets sent after a while
#[test]
fn ack_gets_sent_after_a_while() {
    let mut a = End::new();
    let mut b = End::new();
    deliver(&a.protocol.send(b"a1".to_vec()), &mut b);
    assert_eq!(b.messages.pop().unwrap(), b"a1");
    assert_eq!(a.protocol.unacknowledged(), 1);
    // Nothing to piggyback on: b owes an explicit ack (upstream sends it 2s later).
    assert!(b.protocol.ack_due());
    let ack = b.protocol.make_ack();
    assert_eq!(ack.frame_type(), Some(FrameType::Ack));
    assert_eq!(ack.ack, 1);
    assert!(!b.protocol.ack_due());
    deliver(&ack, &mut a);
    assert_eq!(a.protocol.unacknowledged(), 0);
}

#[test]
fn header_layout_is_13_bytes_big_endian() {
    let f = Frame::new(FrameType::Regular, 0x0102_0304, 0x0A0B_0C0D, vec![9, 9]);
    assert_eq!(
        f.encode(),
        [1, 1, 2, 3, 4, 10, 11, 12, 13, 0, 0, 0, 2, 9, 9]
    );
    assert_eq!(HEADER_LENGTH, 13);
}

#[test]
fn frames_split_across_reads_are_reassembled() {
    let mut a = Protocol::new();
    let bytes: Vec<u8> = [a.send(b"hello".to_vec()), a.send(b"world!".to_vec())]
        .iter()
        .flat_map(Frame::encode)
        .collect();
    let mut reader = FrameReader::new();
    let mut got = vec![];
    for byte in &bytes {
        got.extend(reader.push(std::slice::from_ref(byte)));
    }
    assert_eq!(
        got.iter().map(|f| f.data.clone()).collect::<Vec<_>>(),
        vec![b"hello".to_vec(), b"world!".to_vec()]
    );
    assert_eq!(reader.buffered(), 0);
    // Two frames in one read.
    assert_eq!(FrameReader::new().push(&bytes).len(), 2);
}

#[test]
fn a_gap_asks_for_a_replay_and_replay_resends_the_unacked() {
    let mut a = End::new();
    let mut b = End::new();
    let f1 = a.protocol.send(b"1".to_vec());
    let f2 = a.protocol.send(b"2".to_vec());
    let _ = f1; // lost on the wire
    let back = b.receive(&f2.encode());
    assert!(b.messages.is_empty());
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].frame_type(), Some(FrameType::ReplayRequest));
    // a answers the replay request with everything unacknowledged.
    let resent = a.receive(&back[0].encode());
    assert!(
        resent
            .iter()
            .map(|f| f.data.clone())
            .eq([b"1".to_vec(), b"2".to_vec()])
    );
    for f in &resent {
        b.receive(&f.encode());
    }
    assert_eq!(b.messages, vec![b"1".to_vec(), b"2".to_vec()]);
}

#[test]
fn duplicate_and_unknown_frames_are_ignored() {
    let mut a = End::new();
    let mut b = End::new();
    let f = a.protocol.send(b"x".to_vec());
    deliver(&f, &mut b);
    deliver(&f, &mut b); // a replayed duplicate
    assert_eq!(b.messages.len(), 1);
    let unknown = Frame {
        ty: 42,
        id: 0,
        ack: 0,
        data: vec![1],
    };
    deliver(&unknown, &mut b);
    assert_eq!(b.messages.len(), 1);
}

#[test]
fn pause_and_resume_are_tracked() {
    let mut p = Protocol::new();
    assert!(!p.peer_paused());
    p.receive(Frame::new(FrameType::Pause, 0, 0, vec![]));
    assert!(p.peer_paused());
    p.receive(Frame::new(FrameType::Resume, 0, 0, vec![]));
    assert!(!p.peer_paused());
}
