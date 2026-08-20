use super::*;

#[test]
fn counts_bytes_against_the_announced_total() {
    assert_eq!(progress(512, 2048), "512/2048 bytes (25%)");
}

/// An announce with no total still has to print something, and a bare count is
/// the one thing that cannot be wrong.
#[test]
fn drops_the_percentage_when_there_is_no_total() {
    assert_eq!(progress(512, 0), "512 bytes");
}

#[test]
fn prints_a_sub_second_transfer_in_hundredths() {
    assert_eq!(elapsed(Duration::from_millis(420)), "0.42s");
}

/// Two decimals throughout, so a column of times lines up.
#[test]
fn keeps_the_precision_as_the_seconds_grow() {
    assert_eq!(elapsed(Duration::from_millis(12_300)), "12.30s");
}

#[test]
fn breaks_a_long_transfer_into_minutes_and_seconds() {
    assert_eq!(elapsed(Duration::from_secs(125)), "2m 05s");
}

/// The minute boundary is where the two arms meet, and a time that rounds up to
/// the minute belongs to the minutes arm — otherwise it prints `60.00s`.
#[test]
fn switches_to_minutes_at_exactly_one_minute() {
    assert_eq!(elapsed(Duration::from_millis(59_990)), "59.99s");
    assert_eq!(elapsed(Duration::from_millis(59_999)), "1m 00s");
    assert_eq!(elapsed(Duration::from_secs(60)), "1m 00s");
}

/// A transfer that has not moved yet still prints a time, so the line never
/// arrives with a hole in it.
#[test]
fn prints_a_zero_duration() {
    assert_eq!(elapsed(Duration::ZERO), "0.00s");
}
