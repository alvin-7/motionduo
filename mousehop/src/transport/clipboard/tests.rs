use super::*;

fn exchange(a: &mut Session, b: &mut Session, now: u32) {
    a.tick(now).unwrap();
    b.tick(now).unwrap();
    while let Some(packet) = a.packet() {
        b.input(&packet).unwrap();
    }
    while let Some(packet) = b.packet() {
        a.input(&packet).unwrap();
    }
}

#[test]
fn bidirectional_chunks_survive_data_and_ack_loss_without_duplicates() {
    let mut a = Session::new(7, 0).unwrap();
    let mut b = Session::new(7, 0).unwrap();
    let chunk = vec![42; MAX_MESSAGE];
    a.send(&chunk).unwrap();
    b.send(&chunk).unwrap();
    a.tick(10).unwrap();
    b.tick(10).unwrap();
    // Lose the entire first flight in both directions, including any ACKs.
    while a.packet().is_some() {}
    while b.packet().is_some() {}
    assert_eq!(a.receive().unwrap(), None);
    assert_eq!(b.receive().unwrap(), None);
    let mut received_a = Vec::new();
    let mut received_b = Vec::new();
    let mut lost_ack = false;
    for now in (20..5000).step_by(10) {
        a.tick(now).unwrap();
        b.tick(now).unwrap();
        while let Some(packet) = a.packet() {
            b.input(&packet).unwrap();
        }
        while let Some(packet) = b.packet() {
            if !lost_ack && packet[4] == 82 {
                lost_ack = true;
            } else {
                a.input(&packet).unwrap();
            }
        }
        while let Some(bytes) = a.receive().unwrap() {
            received_a.push(bytes);
        }
        while let Some(bytes) = b.receive().unwrap() {
            received_b.push(bytes);
        }
    }
    assert!(lost_ack);
    assert_eq!(received_a, vec![chunk.clone()]);
    assert_eq!(received_b, vec![chunk]);
    assert_eq!(a.kcp.as_ref().unwrap().wait_snd(), 0);
    assert_eq!(b.kcp.as_ref().unwrap().wait_snd(), 0);
}

#[test]
fn message_and_fragment_budgets_reject_atomically_and_resume() {
    let mut a = Session::new(7, 0).unwrap();
    let mut b = Session::new(7, 0).unwrap();
    assert_eq!(a.send(&[]), Err(Error::MessageSize));
    assert_eq!(a.send(&vec![0; MAX_MESSAGE + 1]), Err(Error::MessageSize));
    let chunk = vec![42; MAX_MESSAGE];
    a.send(&chunk).unwrap();
    a.send(&chunk).unwrap();
    let pending = a.kcp.as_ref().unwrap().wait_snd();
    assert_eq!(pending, 2 * MAX_FRAGMENTS);
    assert_eq!(a.send(&chunk), Err(Error::Backpressure));
    assert_eq!(a.kcp.as_ref().unwrap().wait_snd(), pending);
    for now in (10..1000).step_by(10) {
        exchange(&mut a, &mut b, now);
    }
    assert_eq!(b.receive().unwrap(), Some(chunk.clone()));
    assert_eq!(b.receive().unwrap(), Some(chunk.clone()));
    assert_eq!(b.receive().unwrap(), None);
    a.send(&chunk).unwrap();
}

#[test]
fn stalled_consumer_is_bounded_and_delivery_resumes_in_order() {
    let mut a = Session::new(7, 0).unwrap();
    let mut b = Session::new(7, 0).unwrap();
    let mut sent = 0u32;
    let mut blocked = false;
    for now in (10..3000).step_by(10) {
        loop {
            match a.send(&sent.to_le_bytes()) {
                Ok(()) => sent += 1,
                Err(Error::Backpressure) => {
                    blocked = true;
                    break;
                }
                other => panic!("unexpected send: {other:?}"),
            }
        }
        exchange(&mut a, &mut b, now);
        assert!(b.incoming.len() <= RECEIVE_MESSAGES);
        assert!(a.kcp.as_ref().unwrap().wait_snd() <= SEND_SEGMENTS);
        assert!(a.output.0.borrow().len() <= OUTPUT_PACKETS);
    }
    assert!(blocked);
    // Two receive windows plus the app queue and bounded sender, not elapsed time.
    assert!(sent as usize <= RECEIVE_MESSAGES + 2 * RECEIVE_WINDOW as usize + SEND_SEGMENTS);
    let mut received = 0u32;
    for now in (3000..15000).step_by(10) {
        exchange(&mut a, &mut b, now);
        while let Some(bytes) = b.receive().unwrap() {
            assert_eq!(bytes, received.to_le_bytes());
            received += 1;
        }
        if received == sent {
            break;
        }
    }
    assert_eq!(received, sent);
    a.send(b"resumed").unwrap();
}

#[test]
fn malformed_bulk_and_output_overflow_fail_only_their_own_session() {
    // Exercise the real input core too, not merely another bulk instance.
    let mut input_a =
        super::super::core::Session::recovered(true, 42, 8, 0, super::super::Timeouts::default())
            .unwrap();
    let mut input_b =
        super::super::core::Session::recovered(false, 42, 8, 0, super::super::Timeouts::default())
            .unwrap();
    let mut healthy_a = Session::new(8, 0).unwrap();
    let mut healthy_b = Session::new(8, 0).unwrap();
    let mut broken = Session::new(7, 0).unwrap();
    assert_eq!(broken.input(&[0]), Err(Error::Datagram));
    assert!(broken.kcp.is_none());
    assert_eq!(broken.send(b"closed"), Err(Error::Closed));
    assert_eq!(broken.receive(), Err(Error::Closed));
    let mut overflow = Session::new(9, 0).unwrap();
    // Valid duplicate PUSH packets grow ACK state unless flushed and bounded.
    let mut packet = vec![0; HEADER + 1];
    packet[..4].copy_from_slice(&9u32.to_le_bytes());
    packet[4] = 81;
    packet[6..8].copy_from_slice(&128u16.to_le_bytes());
    packet[20..24].copy_from_slice(&1u32.to_le_bytes());
    for _ in 0..(OUTPUT_PACKETS + 1) * (MTU / HEADER) {
        overflow.input(&packet).unwrap();
    }
    assert_eq!(overflow.input(&packet), Err(Error::Failed));
    assert!(overflow.kcp.is_none());
    assert!(overflow.packet().is_none());
    input_a.send(b"key-release", 10).unwrap();
    for bytes in input_a.drain() {
        input_b.input(&bytes, 10).unwrap();
    }
    let (sequence, payload) = input_b.receive().unwrap();
    assert_eq!(payload, b"key-release");
    input_b.processed(sequence, 10).unwrap();
    assert!(input_a.ready() && input_b.ready());
    healthy_a.send(b"input-independent").unwrap();
    for now in (10..100).step_by(10) {
        exchange(&mut healthy_a, &mut healthy_b, now);
    }
    assert_eq!(
        healthy_b.receive().unwrap(),
        Some(b"input-independent".to_vec())
    );
}

#[test]
fn rejects_wrong_conversation_trailing_data_and_excessive_fragments() {
    let mut sender = Session::new(7, 0).unwrap();
    sender.send(b"payload").unwrap();
    sender.tick(10).unwrap();
    let packet = sender.packet().unwrap();
    let mut bad_packets = vec![packet.clone(), packet.clone(), packet.clone()];
    bad_packets[0][0] ^= 1;
    bad_packets[1].push(0);
    bad_packets[2][5] = MAX_FRAGMENTS as u8;
    for bad in bad_packets {
        let mut receiver = Session::new(7, 0).unwrap();
        assert_eq!(receiver.input(&bad), Err(Error::Datagram));
        assert!(receiver.kcp.is_none());
    }
}
