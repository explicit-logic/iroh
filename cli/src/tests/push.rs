use super::*;

fn announce() -> Announce {
    Announce {
        hash: Hash::from_bytes([7; 32]),
        format: BlobFormat::HashSeq,
        total_size: 4096,
        file_count: 3,
        is_dir: true,
        name: "photos".to_string(),
    }
}

#[test]
fn round_trips_a_frame() {
    let expected = announce();

    let decoded = Announce::decode(&expected.encode().unwrap()).unwrap();

    assert_eq!(decoded, expected);
}

/// The frame is at most 303 bytes and the receiver reads no more than that, so a
/// name at the cap must still fit.
#[test]
fn round_trips_a_name_at_the_cap() {
    let expected = Announce {
        name: "n".repeat(MAX_NAME_LEN),
        ..announce()
    };

    let encoded = expected.encode().unwrap();

    assert_eq!(encoded.len(), MAX_ANNOUNCE_LEN);
    assert_eq!(Announce::decode(&encoded).unwrap(), expected);
}

#[test]
fn rejects_a_name_over_the_cap() {
    let err = Announce {
        name: "n".repeat(MAX_NAME_LEN + 1),
        ..announce()
    }
    .encode()
    .unwrap_err()
    .to_string();

    assert!(err.contains("limit"), "unexpected error: {err}");
}

#[test]
fn rejects_an_empty_name() {
    let err = Announce {
        name: String::new(),
        ..announce()
    }
    .encode()
    .unwrap_err()
    .to_string();

    assert!(err.contains("empty"), "unexpected error: {err}");
}

#[test]
fn rejects_a_truncated_frame() {
    let encoded = announce().encode().unwrap();

    assert!(Announce::decode(&encoded[..encoded.len() - 1]).is_err());
    assert!(Announce::decode(&encoded[..10]).is_err());
    assert!(Announce::decode(&[]).is_err());
}

/// A `name_len` larger than the bytes that follow it is the obvious way to try to
/// make the receiver read past its buffer.
#[test]
fn rejects_a_name_len_past_the_end() {
    let mut encoded = announce().encode().unwrap();
    encoded[46] = 0xff;
    encoded[47] = 0x00;

    assert!(Announce::decode(&encoded).is_err());
}

#[test]
fn rejects_trailing_bytes_after_the_name() {
    let mut encoded = announce().encode().unwrap();
    encoded.push(b'!');

    assert!(Announce::decode(&encoded).is_err());
}

#[test]
fn rejects_a_non_utf8_name() {
    let mut encoded = announce().encode().unwrap();
    let last = encoded.len() - 1;
    encoded[last] = 0xff;

    assert!(Announce::decode(&encoded).is_err());
}

#[test]
fn rejects_an_unknown_format_byte() {
    let mut encoded = announce().encode().unwrap();
    encoded[32] = 9;

    assert!(Announce::decode(&encoded).is_err());
}

#[test]
fn rejects_an_unknown_is_dir_byte() {
    let mut encoded = announce().encode().unwrap();
    encoded[45] = 2;

    assert!(Announce::decode(&encoded).is_err());
}
