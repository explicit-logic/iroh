//! The progress line both sides print, so a transfer reads the same whichever
//! end you are watching.

use std::time::Duration;

/// How long a side waits between progress lines. Both sides use it, so neither
/// scrolls the other off the screen.
pub const PROGRESS_INTERVAL: Duration = Duration::from_millis(500);

/// The byte count in `done/total (pct)` form.
///
/// `total` comes from the announce and counts file bytes only, while `done`
/// counts every payload byte on the wire — the collection blob included — so the
/// percentage can nudge past 100 on a tiny transfer. That is cosmetic, and the
/// alternative is a count that does not match the bytes actually moved.
pub fn progress(done: u64, total: u64) -> String {
    match total {
        0 => format!("{done} bytes"),
        total => format!(
            "{done}/{total} bytes ({}%)",
            done.saturating_mul(100) / total
        ),
    }
}

pub fn elapsed(duration: Duration) -> String {
    let hundredths = (duration.as_secs_f64() * 100.0).round() as u64;
    let seconds = hundredths / 100;
    match hundredths {
        0..6000 => format!("{seconds}.{:02}s", hundredths % 100),
        _ => format!("{}m {:02}s", seconds / 60, seconds % 60),
    }
}

#[cfg(test)]
#[path = "tests/progress.rs"]
mod tests;
