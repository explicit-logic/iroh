use std::time::Duration;

use iroh_blobs::{
    Hash,
    provider::{
        TransferStats,
        events::{TransferCompleted, TransferProgress, TransferStarted},
    },
};

use super::*;

fn started(index: u64, size: u64) -> RequestUpdate {
    RequestUpdate::Started(TransferStarted {
        index,
        hash: Hash::from_bytes([index as u8; 32]),
        size,
    })
}

fn sent(end_offset: u64) -> RequestUpdate {
    RequestUpdate::Progress(TransferProgress { end_offset })
}

fn completed(payload_bytes_sent: u64) -> RequestUpdate {
    RequestUpdate::Completed(TransferCompleted {
        stats: Box::new(TransferStats {
            payload_bytes_sent,
            other_bytes_sent: 7,
            other_bytes_read: 11,
            duration: Duration::from_secs(1),
        }),
    })
}

/// A single blob's offsets are already the count, so they pass straight through.
#[test]
fn counts_one_blob_by_its_offset() {
    let mut sending = Sent::default();

    sending.apply(&started(0, 100));

    assert_eq!(sending.apply(&sent(40)), 40);
    assert_eq!(sending.apply(&sent(100)), 100);
}

/// The offsets restart at zero for every blob, so a directory only reads right if
/// the finished ones are carried forward.
#[test]
fn carries_finished_blobs_forward() {
    let mut sending = Sent::default();

    sending.apply(&started(0, 100));
    sending.apply(&sent(100));
    sending.apply(&started(1, 50));

    assert_eq!(
        sending.apply(&sent(20)),
        120,
        "the second blob's offset should sit on top of the first blob"
    );
}

/// A blob whose last chunk is not reported still counts what it did send, or the
/// running total would drop back on the next `Started`.
#[test]
fn keeps_the_bytes_of_a_blob_with_no_final_offset() {
    let mut sending = Sent::default();

    sending.apply(&started(0, 100));
    sending.apply(&sent(80));

    assert_eq!(sending.apply(&started(1, 50)), 80);
}

/// Progress is dropped rather than queued when the channel is full, so the
/// offsets seen can lag the transfer. The provider's own stats settle it.
#[test]
fn ends_on_the_provider_stats() {
    let mut sending = Sent::default();

    sending.apply(&started(0, 1000));
    sending.apply(&sent(200));

    assert_eq!(sending.apply(&completed(1000)), 1000);
}
