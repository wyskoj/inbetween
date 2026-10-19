//! Which frame plays next. Pure, so the rules are testable without a window.

/// Where playback is: the frame shown and whether it is moving toward the last frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub frame: u32,
    pub forward: bool,
}

/// Playback options.
#[derive(Clone, Copy, Debug)]
pub struct Mode {
    /// Start again after the end (after a full there-and-back in ping-pong).
    pub looping: bool,
    /// Play forward, then backward, instead of jumping back to the start.
    pub ping_pong: bool,
}

/// Move one frame. Returns the new cursor, and whether playback goes on (false when it ran
/// out of frames and is not looping). `frames` is the total, hold frames included.
pub fn step(at: Cursor, frames: u32, mode: Mode) -> (Cursor, bool) {
    let last = frames.saturating_sub(1);
    let frame = at.frame.min(last);
    if frames < 2 {
        return (
            Cursor {
                frame: 0,
                forward: true,
            },
            false,
        );
    }
    if mode.ping_pong {
        if at.forward {
            if frame < last {
                return (
                    Cursor {
                        frame: frame + 1,
                        forward: true,
                    },
                    true,
                );
            }
            return (
                Cursor {
                    frame: last - 1,
                    forward: false,
                },
                true,
            );
        }
        if frame > 0 {
            let next = Cursor {
                frame: frame - 1,
                forward: false,
            };
            // Arriving back at the start ends a round trip.
            return if frame == 1 && !mode.looping {
                (next, false)
            } else {
                (next, true)
            };
        }
        return if mode.looping {
            (
                Cursor {
                    frame: 1,
                    forward: true,
                },
                true,
            )
        } else {
            (
                Cursor {
                    frame: 0,
                    forward: true,
                },
                false,
            )
        };
    }
    if frame < last {
        (
            Cursor {
                frame: frame + 1,
                forward: true,
            },
            true,
        )
    } else if mode.looping {
        (
            Cursor {
                frame: 0,
                forward: true,
            },
            true,
        )
    } else {
        (
            Cursor {
                frame: last,
                forward: true,
            },
            false,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(frames: u32, mode: Mode, start: Cursor, steps: usize) -> Vec<u32> {
        let mut at = start;
        let mut seen = vec![at.frame];
        for _ in 0..steps {
            let (next, go) = step(at, frames, mode);
            at = next;
            seen.push(at.frame);
            if !go {
                break;
            }
        }
        seen
    }

    const FWD: Cursor = Cursor {
        frame: 0,
        forward: true,
    };

    #[test]
    fn plain_loop_wraps() {
        let m = Mode {
            looping: true,
            ping_pong: false,
        };
        assert_eq!(run(3, m, FWD, 6), [0, 1, 2, 0, 1, 2, 0]);
    }

    #[test]
    fn plain_play_stops_on_the_last_frame() {
        let m = Mode {
            looping: false,
            ping_pong: false,
        };
        assert_eq!(run(3, m, FWD, 10), [0, 1, 2, 2]);
    }

    #[test]
    fn ping_pong_loops_without_repeating_the_end_frames() {
        let m = Mode {
            looping: true,
            ping_pong: true,
        };
        assert_eq!(run(4, m, FWD, 9), [0, 1, 2, 3, 2, 1, 0, 1, 2, 3]);
    }

    #[test]
    fn ping_pong_without_loop_stops_back_at_the_start() {
        let m = Mode {
            looping: false,
            ping_pong: true,
        };
        assert_eq!(run(4, m, FWD, 20), [0, 1, 2, 3, 2, 1, 0]);
    }

    #[test]
    fn degenerate_lengths_stop() {
        for ping_pong in [false, true] {
            let m = Mode {
                looping: true,
                ping_pong,
            };
            assert!(!step(FWD, 1, m).1);
            assert!(!step(FWD, 0, m).1);
        }
        // Two frames bounce cleanly.
        let m = Mode {
            looping: true,
            ping_pong: true,
        };
        assert_eq!(run(2, m, FWD, 4), [0, 1, 0, 1, 0]);
    }

    #[test]
    fn a_frame_past_the_end_is_pulled_back() {
        let m = Mode {
            looping: false,
            ping_pong: false,
        };
        assert_eq!(
            step(
                Cursor {
                    frame: 99,
                    forward: true
                },
                3,
                m
            )
            .0
            .frame,
            2
        );
    }
}
