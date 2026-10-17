//! Keeping a guest's player in step with the host's.

use crate::protocol::Item;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// A gap bigger than this is fixed with a seek; a smaller one by running a little faster or slower.
const SEEK_ABOVE: f64 = 0.8;
/// A gap smaller than this is not worth correcting.
const LEAVE_BELOW: f64 = 0.15;
/// How much faster or slower to run to close a small gap, which is not noticeable.
const NUDGE: f64 = 0.05;
/// However long ago the host's clock says it spoke, a guest moves its word on by no more than
/// this: a clock that is far out must not send a guest far from the picture.
const MAX_LATE: f64 = 10.;

/// What is playing, and how far in.
#[derive(Clone, Debug, PartialEq)]
pub struct Playback {
    pub item: Option<Item>,
    pub playing: bool,
    pub position: f64,
}

/// What a guest should be playing: the host's `Playback`, with the position where the host is
/// now, as best it can be told.
#[derive(Clone, Debug)]
pub struct Target {
    pub playback: Playback,
    received: Instant,
}

impl Target {
    pub(crate) fn new(playback: Playback) -> Target {
        Target {
            playback,
            received: Instant::now(),
        }
    }

    /// Where the host is now: the position it reported, moved on by the time since.
    pub fn position_now(&self) -> f64 {
        if self.playback.playing {
            self.playback.position + self.received.elapsed().as_secs_f64()
        } else {
            self.playback.position
        }
    }
}

/// What a guest does to its player to catch up: a seek, or a playback speed (1.0 is normal).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fix {
    pub seek: Option<f64>,
    pub speed: f64,
}

/// How to bring a guest at `local` seconds to where the host is, `target`.
pub fn fix(local: f64, target: f64, playing: bool) -> Fix {
    let gap = local - target;
    let paused = Fix {
        seek: (gap.abs() > LEAVE_BELOW).then_some(target),
        speed: 1.0,
    };
    if !playing {
        return paused;
    }
    if gap.abs() > SEEK_ABOVE {
        return Fix {
            seek: Some(target),
            speed: 1.0,
        };
    }
    Fix {
        seek: None,
        speed: if gap.abs() < LEAVE_BELOW {
            1.0
        } else if gap > 0.0 {
            1.0 - NUDGE
        } else {
            1.0 + NUDGE
        },
    }
}

pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}

/// The difference between this device's clock and the host's, from the quickest round trip seen
/// (the quickest is the least disturbed, so the closest to the truth).
#[derive(Default)]
pub(crate) struct Clock {
    best: Option<(u64, i64)>,
}

impl Clock {
    /// A `Pong`: sent at `sent` (this clock), answered at `at` (host clock), received at `now`.
    pub fn sample(&mut self, sent: u64, at: u64, now: u64) {
        let round_trip = now.saturating_sub(sent);
        // The host read its clock about half way.
        let offset = at as i64 + (round_trip / 2) as i64 - now as i64;
        if self.best.is_none_or(|(best, _)| round_trip <= best) {
            self.best = Some((round_trip, offset));
        }
    }

    /// Whether a `Pong` has been seen, so the clocks are known.
    pub fn is_known(&self) -> bool {
        self.best.is_some()
    }

    /// The host's clock now, when this device's reads `now`.
    pub fn host_time(&self, now: u64) -> u64 {
        let offset = self.best.map_or(0, |(_, offset)| offset);
        (now as i64 + offset).max(0) as u64
    }
}

/// What the host said, moved on by how long ago it said it.
pub(crate) fn target_of(
    item: Option<Item>,
    playing: bool,
    position: f64,
    at: u64,
    clock: &Clock,
    now: u64,
) -> Target {
    let late = (clock.host_time(now).saturating_sub(at) as f64 / 1000.0).min(MAX_LATE);
    Target::new(Playback {
        item,
        playing,
        position: if playing { position + late } else { position },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_small_gap_is_closed_by_changing_speed_and_a_big_one_by_seeking() {
        assert_eq!(
            fix(10.0, 10.05, true),
            Fix {
                seek: None,
                speed: 1.0
            }
        );
        assert_eq!(
            fix(10.0, 10.5, true),
            Fix {
                seek: None,
                speed: 1.05
            }
        );
        assert_eq!(
            fix(10.5, 10.0, true),
            Fix {
                seek: None,
                speed: 0.95
            }
        );
        assert_eq!(
            fix(10.0, 14.0, true),
            Fix {
                seek: Some(14.0),
                speed: 1.0
            }
        );
        assert_eq!(
            fix(14.0, 10.0, true),
            Fix {
                seek: Some(10.0),
                speed: 1.0
            }
        );
    }

    #[test]
    fn a_paused_party_is_matched_exactly_at_normal_speed() {
        assert_eq!(
            fix(10.0, 10.5, false),
            Fix {
                seek: Some(10.5),
                speed: 1.0
            }
        );
        assert_eq!(
            fix(10.0, 10.1, false),
            Fix {
                seek: None,
                speed: 1.0
            }
        );
    }

    #[test]
    fn the_clock_is_read_from_the_quickest_round_trip() {
        let mut clock = Clock::default();
        // The host's clock is 5000 ms ahead. A slow trip, then a fast one.
        clock.sample(1000, 6300, 1600);
        clock.sample(2000, 7010, 2020);
        assert_eq!(clock.host_time(3000), 8000);
    }

    #[test]
    fn a_state_is_moved_on_by_the_time_it_took_to_arrive() {
        let mut clock = Clock::default();
        clock.sample(1000, 6010, 1020);
        // The host said "10.0 seconds" at its 7000; it is now its 7400 (this device's 2400).
        let target = target_of(None, true, 10.0, 7000, &clock, 2400);
        assert!((target.playback.position - 10.4).abs() < 0.001);
        let paused = target_of(None, false, 10.0, 7000, &clock, 2400);
        assert_eq!(paused.playback.position, 10.0);
    }

    #[test]
    fn a_clock_that_is_far_out_cannot_send_a_guest_far_away() {
        // No reading yet, and the host's clock is ten minutes behind this one.
        let target = target_of(None, true, 10.0, 0, &Clock::default(), 600_000);
        assert_eq!(target.playback.position, 20.0);
    }
}
