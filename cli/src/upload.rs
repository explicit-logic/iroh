//! The serving half of a push, and the progress it prints.
//!
//! iroh-blobs is a pull protocol, so the sender has no loop of its own to count
//! bytes in: it hands the connection to [`BlobsProtocol`] and the receiver drives
//! the transfer. The byte counts therefore come from the provider's own event
//! stream rather than from the send path.

use std::time::Instant;

use anyhow::{Context, Result};
use iroh::{endpoint::Connection, protocol::ProtocolHandler};
use iroh_blobs::{
    BlobsProtocol,
    api::Store,
    provider::events::{
        ConnectMode, EventMask, EventSender, ObserveMode, ProviderMessage, RequestMode,
        RequestUpdate, ThrottleMode,
    },
};

use crate::{
    progress::{PROGRESS_INTERVAL, elapsed, progress},
    push::Announce,
};

/// Room for the provider's events. Only the mandatory ones block the transfer —
/// progress is dropped rather than queued — so this only has to be deep enough
/// that the start of a blob is not waiting on us.
const EVENT_CAPACITY: usize = 32;

/// Serves the collection on `conn` until the receiver is done with it, printing
/// progress as the bytes go out.
pub async fn serve_blobs(store: &Store, conn: &Connection, announce: &Announce) -> Result<()> {
    let (events, updates) = EventSender::channel(EVENT_CAPACITY, event_mask());
    let blobs = BlobsProtocol::new(store, Some(events));

    // The tracker has to run alongside the transfer, and it ends when the last
    // `EventSender` is dropped — which is `blobs`, below.
    let name = announce.name.clone();
    let total_size = announce.total_size;
    let tracker = tokio::spawn(async move { track(updates, &name, total_size).await });

    let served = blobs.accept(conn.clone()).await;
    drop(blobs);
    tracker.await.context("the progress task panicked")?;

    served.context("failed to serve the collection")
}

/// Asks for transfer events and nothing else.
///
/// `Notify` rather than `Intercept` throughout: this connection was dialed for
/// this one transfer, so there is nothing to authorise. Push stays disabled — the
/// sender's store is a temp store that must not be written to from the wire.
fn event_mask() -> EventMask {
    EventMask {
        connected: ConnectMode::None,
        get: RequestMode::NotifyLog,
        get_many: RequestMode::None,
        push: RequestMode::Disabled,
        observe: ObserveMode::None,
        throttle: ThrottleMode::None,
    }
}

/// Prints a progress line for each request the receiver makes, and a closing line
/// with the time the request took.
///
/// The time comes from the provider's own stats rather than from a clock here:
/// the provider starts counting when it reads the request, which is the point the
/// transfer begins, and this task only learns of it once the first event arrives.
///
/// The requests are handled one after another rather than concurrently: a push is
/// a single get request, and two overlapping counters would print two
/// interleaved sets of progress lines for one transfer.
async fn track(mut updates: tokio::sync::mpsc::Receiver<ProviderMessage>, name: &str, total: u64) {
    while let Some(message) = updates.recv().await {
        let ProviderMessage::GetRequestReceivedNotify(request) = message else {
            continue;
        };

        let mut rx = request.rx;
        let mut sent = Sent::default();
        let mut last_printed = Instant::now();

        while let Ok(Some(update)) = rx.recv().await {
            let done = sent.apply(&update);
            match update {
                RequestUpdate::Progress(_) if last_printed.elapsed() >= PROGRESS_INTERVAL => {
                    println!("sending {name}: {}", progress(done, total));
                    last_printed = Instant::now();
                }
                RequestUpdate::Completed(completed) => {
                    println!(
                        "sent {name}: {} in {}",
                        progress(done, total),
                        elapsed(completed.stats.duration)
                    );
                    break;
                }
                RequestUpdate::Aborted(aborted) => {
                    println!(
                        "gave up sending {name} after {}: {}",
                        elapsed(aborted.stats.duration),
                        progress(done, total)
                    );
                    break;
                }
                _ => {}
            }
        }
    }
}

/// Turns the provider's per-blob offsets into one byte count for the request.
///
/// The provider reports progress as an offset within the blob it is sending, and
/// starts again from zero at the next one, so the offsets have to be added up to
/// mean anything to someone watching a directory go out.
#[derive(Debug, Default)]
struct Sent {
    /// The blobs already finished.
    finished: u64,
    /// How far into the blob in flight we are.
    current: u64,
}

impl Sent {
    /// Folds `update` in and returns the bytes sent so far.
    fn apply(&mut self, update: &RequestUpdate) -> u64 {
        match update {
            // The previous blob is done, however far it got: the provider sends no
            // final offset for it.
            RequestUpdate::Started(_) => {
                self.finished += self.current;
                self.current = 0;
            }
            RequestUpdate::Progress(progress) => self.current = progress.end_offset,
            // The stats cover the whole request, so they replace the running count
            // rather than adding to it.
            RequestUpdate::Completed(completed) => {
                self.finished = completed.stats.payload_bytes_sent;
                self.current = 0;
            }
            RequestUpdate::Aborted(aborted) => {
                self.finished = aborted.stats.payload_bytes_sent;
                self.current = 0;
            }
        }
        self.finished + self.current
    }
}

#[cfg(test)]
#[path = "tests/upload.rs"]
mod tests;
