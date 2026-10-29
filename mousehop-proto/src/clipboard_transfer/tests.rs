use super::*;

#[test]
fn session_v2_envelope_is_bounded_and_incompatible_with_bare_v1() {
    for frame in [
        SessionFrame::Generation { token: 1 },
        SessionFrame::Data {
            token: 2,
            source: 3,
            message: Message::Chunk {
                id: 4,
                offset: 0,
                payload: vec![7; SESSION_CHUNK_SIZE],
            },
        },
    ] {
        let bytes = frame.encode().unwrap();
        assert!(bytes.len() <= MAX_MESSAGE_SIZE);
        assert_eq!(SessionFrame::decode(&bytes).unwrap(), frame);
        assert!(Message::decode(&bytes).is_err());
        for end in 0..bytes.len() {
            assert!(SessionFrame::decode(&bytes[..end]).is_err());
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(SessionFrame::decode(&trailing).is_err());
        let mut wrong_version = bytes;
        wrong_version[4] = 1;
        assert_eq!(
            SessionFrame::decode(&wrong_version),
            Err(CodecError::UnsupportedVersion(1))
        );
    }
    assert!(SessionFrame::decode(&Message::Complete { id: 1 }.encode().unwrap()).is_err());
    assert!(SessionFrame::Generation { token: 0 }.encode().is_err());
    assert!(
        SessionFrame::Data {
            token: 1,
            source: 0,
            message: Message::Complete { id: 1 }
        }
        .encode()
        .is_err()
    );
    assert!(
        SessionFrame::Data {
            token: 1,
            source: 1,
            message: Message::Chunk {
                id: 1,
                offset: 0,
                payload: vec![7; SESSION_CHUNK_SIZE + 1]
            }
        }
        .encode()
        .is_err()
    );
}

fn frames() -> Vec<Message> {
    let mut frames = vec![
        Message::Offer {
            id: 1,
            format: Format::Text,
            size: MAX_TEXT_SIZE as u32,
        },
        Message::Offer {
            id: u64::MAX,
            format: Format::Png,
            size: MAX_PNG_SIZE as u32,
        },
        Message::Chunk {
            id: 1,
            offset: 0,
            payload: vec![42],
        },
        Message::Complete { id: 1 },
    ];
    for text in [false, true] {
        for png in [false, true] {
            frames.push(Message::Permission { text, png });
        }
    }
    for reason in [
        CancelReason::PermissionDenied,
        CancelReason::Superseded,
        CancelReason::InvalidData,
        CancelReason::TooLarge,
        CancelReason::Aborted,
    ] {
        frames.push(Message::Cancel { id: 1, reason });
    }
    frames
}

#[test]
fn round_trip_rejects_every_truncation_and_trailing_data() {
    for frame in frames() {
        let bytes = frame.encode().unwrap();
        assert_eq!(Message::decode(&bytes), Ok(frame));
        for end in 0..bytes.len() {
            assert!(Message::decode(&bytes[..end]).is_err(), "prefix {end}");
        }
        let mut trailing = bytes;
        trailing.push(0);
        assert!(Message::decode(&trailing).is_err());
    }
}

#[test]
fn wire_layout_is_stable() {
    assert_eq!(
        Message::Permission {
            text: false,
            png: true
        }
        .encode()
        .unwrap(),
        b"MHCB\x01\x00\x02"
    );
    let bytes = Message::Offer {
        id: 0x0807060504030201,
        format: Format::Png,
        size: MAX_PNG_SIZE as u32,
    }
    .encode()
    .unwrap();
    assert_eq!(
        bytes,
        b"MHCB\x01\x01\x01\x02\x03\x04\x05\x06\x07\x08\x01\x00\x00\x00\x01"
    );
}

#[test]
fn invalid_discriminants_and_zero_ids_are_rejected() {
    for frame in frames() {
        let bytes = frame.encode().unwrap();
        for (index, value) in [(0, 0), (4, VERSION + 1), (5, 255)] {
            let mut bad = bytes.clone();
            bad[index] = value;
            assert!(Message::decode(&bad).is_err());
        }
        if !matches!(frame, Message::Permission { .. }) {
            let mut bad = bytes;
            bad[6..14].fill(0);
            assert_eq!(Message::decode(&bad), Err(CodecError::InvalidId));
        }
    }
    for mask in 4..=255 {
        assert_eq!(
            Message::decode(&[b'M', b'H', b'C', b'B', VERSION, 0, mask]),
            Err(CodecError::InvalidPermission(mask))
        );
    }
    let mut offer = Message::Offer {
        id: 1,
        format: Format::Png,
        size: 1,
    }
    .encode()
    .unwrap();
    offer[14] = 2;
    assert_eq!(Message::decode(&offer), Err(CodecError::InvalidFormat(2)));
    let mut cancel = Message::Cancel {
        id: 1,
        reason: CancelReason::Aborted,
    }
    .encode()
    .unwrap();
    cancel[14] = 5;
    assert_eq!(
        Message::decode(&cancel),
        Err(CodecError::InvalidCancelReason(5))
    );
}

#[test]
fn offer_limits_apply_to_encode_and_decode() {
    for format in [Format::Text, Format::Png] {
        for size in [1, format.max_size() as u32] {
            let frame = Message::Offer {
                id: 1,
                format,
                size,
            };
            assert_eq!(Message::decode(&frame.encode().unwrap()), Ok(frame));
        }
        for size in [0, format.max_size() as u32 + 1, u32::MAX] {
            let bad = Message::Offer {
                id: 1,
                format,
                size,
            };
            assert_eq!(bad.encode(), Err(CodecError::InvalidOfferSize));
            let mut bytes = Message::Offer {
                id: 1,
                format,
                size: 1,
            }
            .encode()
            .unwrap();
            bytes[15..19].copy_from_slice(&size.to_le_bytes());
            assert_eq!(Message::decode(&bytes), Err(CodecError::InvalidOfferSize));
        }
    }
    for frame in [
        Message::Offer {
            id: 0,
            format: Format::Text,
            size: 1,
        },
        Message::Chunk {
            id: 0,
            offset: 0,
            payload: vec![1],
        },
        Message::Complete { id: 0 },
        Message::Cancel {
            id: 0,
            reason: CancelReason::Aborted,
        },
    ] {
        assert_eq!(frame.encode(), Err(CodecError::InvalidId));
    }
}

#[test]
fn chunk_limits_ranges_and_claimed_lengths_are_checked() {
    let frame = Message::Chunk {
        id: 1,
        offset: (MAX_PNG_SIZE - MAX_CHUNK_SIZE) as u32,
        payload: vec![7; MAX_CHUNK_SIZE],
    };
    let bytes = frame.encode().unwrap();
    assert_eq!(bytes.len(), MAX_MESSAGE_SIZE);
    assert_eq!(Message::decode(&bytes), Ok(frame));
    for (offset, length, error) in [
        (0, 0, CodecError::InvalidChunkSize),
        (0, MAX_CHUNK_SIZE + 1, CodecError::InvalidChunkSize),
        (MAX_PNG_SIZE as u32, 1, CodecError::InvalidChunkRange),
        (u32::MAX, 1, CodecError::InvalidChunkRange),
    ] {
        assert_eq!(
            Message::Chunk {
                id: 1,
                offset,
                payload: vec![0; length]
            }
            .encode(),
            Err(error)
        );
        let mut bad = bytes[..22].to_vec();
        bad[14..18].copy_from_slice(&offset.to_le_bytes());
        bad[18..22].copy_from_slice(&(length as u32).to_le_bytes());
        assert_eq!(Message::decode(&bad), Err(error));
    }
    for length in [1, MAX_CHUNK_SIZE as u32, u32::MAX] {
        let mut bad = bytes[..22].to_vec();
        bad[14..18].fill(0);
        bad[18..22].copy_from_slice(&length.to_le_bytes());
        assert!(Message::decode(&bad).is_err());
    }
    assert_eq!(
        Message::decode(&vec![0; MAX_MESSAGE_SIZE + 1]),
        Err(CodecError::MessageTooLarge)
    );
}

#[test]
fn arbitrary_bounded_input_never_panics() {
    let mut state = 0x12345678u32;
    for length in 0..256 {
        let mut bytes = vec![0; length];
        for byte in &mut bytes {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            *byte = state as u8;
        }
        let _ = Message::decode(&bytes);
        if length >= HEADER_SIZE {
            bytes[..4].copy_from_slice(MAGIC);
            bytes[4] = VERSION;
            for kind in 0..=4 {
                bytes[5] = kind;
                let _ = Message::decode(&bytes);
            }
        }
    }
}
