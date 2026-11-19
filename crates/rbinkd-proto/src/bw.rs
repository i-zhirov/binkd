//! The bandwidth rate limit of a session, the port of the `BW_LIM` code of
//! `protocol.c`.
//!
//! The C code meters the bytes of the two directions and holds the descriptor of a
//! direction out of the `select()` set while the direction is over the limit, so the
//! wait is the `select()` timeout. The port holds the meter and waits with the timer
//! of the runtime, so the effect is the same: the bytes of a direction stop until
//! the average falls back to the limit.
//!
//! The C code reads the wall clock, and it resets the meter when the clock steps
//! back. The port reads a monotonic clock, so the time cannot step back.

use std::time::{Duration, Instant};

/// The length of the window of the average in microseconds, `BW_TIME_INT`.
const WINDOW: u64 = 10_000_000;

/// The wait of a direction that sent bytes in the same microsecond, in
/// microseconds, the port of the `0.1 sec` of `check_rate_limit()`.
const SHORT_WAIT: u64 = 100_000;

/// The percent of a node that asks for the full rate, `BW_DEF`.
pub const BW_DEF: i64 = -100;

/// One rule of the `limit-rate` directive, the port of `struct ratechain` of
/// `readcfg.h`.
#[derive(Clone, Debug)]
pub struct RateRule {
    /// The mask of the file names.
    pub mask: String,
    /// The address type of the rule.
    pub atype: i32,
    /// The rate in bytes per second, or a negative percent value.
    pub rate: i64,
}

/// The meter of one direction, the port of `struct _BW` of `protoco2.h`.
#[derive(Clone, Debug, Default)]
pub struct Bandwidth {
    /// The limit of the file in transfer in bytes per second, the port of `rlim`.
    /// Zero asks for no limit.
    pub limit: u64,
    /// The absolute limit of the session in bytes per second, the port of `abs`.
    pub abs: u64,
    /// The percent limit of the session, the port of `rel`.
    pub rel: u64,
    /// The bytes since the last measurement, the port of `bytes`.
    pub bytes: u64,
    /// The average in bytes per second, the port of `cps`.
    pub cps: f64,
    /// The length of the average in microseconds, the port of `cpsN`.
    pub window: u64,
    /// The time of the last measurement, the port of `utime`.
    pub time: Option<Instant>,
}

impl Bandwidth {
    /// Creates a meter with the session limits of a node.
    pub fn new(abs: u64, rel: u64) -> Bandwidth {
        Bandwidth {
            abs,
            rel,
            ..Bandwidth::default()
        }
    }

    /// Starts the meter of a file, the port of `setup_rate_limit()`.
    ///
    /// The C code resets the time of the last measurement only, so the bytes that
    /// arrived before the file count for the first measurement of the file.
    pub fn start(&mut self) {
        self.time = None;
    }

    /// Returns the time to wait, the port of `check_rate_limit()`.
    ///
    /// The answer `None` asks for no wait. The C code lowers the timeout of the
    /// `select()` of the session and it holds the descriptor of the direction out of
    /// the set, so the session waits for the answer of the port.
    pub fn check(&mut self, now: Instant) -> Option<Duration> {
        if self.limit == 0 {
            return None;
        }
        let Some(last) = self.time else {
            // The first check of a file starts the meter, the port of the branch
            // that holds `utime` at zero.
            self.time = Some(now);
            self.cps = 0.0;
            self.window = 0;
            return None;
        };
        let dt = now.duration_since(last).as_micros() as u64;
        if dt == 0 {
            // The bytes arrived in the same microsecond, the port of the branch
            // that compares the seconds and the microseconds.
            if self.bytes > self.limit {
                return Some(Duration::from_micros(SHORT_WAIT));
            }
            return None;
        }
        // The current rate and the average of the C code.
        let cps = self.bytes as f64 * 1_000_000.0 / dt as f64;
        self.bytes = 0;
        self.time = Some(now);
        if self.window < WINDOW {
            self.cps =
                (self.window as f64 * self.cps + cps * dt as f64) / (self.window + dt) as f64;
            self.window += dt;
            if self.window > WINDOW {
                self.window = WINDOW;
            }
        } else if dt >= WINDOW {
            self.cps = cps;
        } else {
            self.cps = ((WINDOW - dt) as f64 * self.cps + cps * dt as f64) / WINDOW as f64;
        }
        if self.cps <= self.limit as f64 {
            return None;
        }
        // The wait of the C code: the window times the relative overshoot.
        let delay = (self.window as f64 * (self.cps / self.limit as f64 - 1.0) + 1000.0) as u64;
        Some(Duration::from_micros(delay))
    }
}

/// Returns the limits of the two directions of a session, the port of the part of
/// `ADR()` that reads the `-bw` of the nodes of the peer.
///
/// The C code reads the nodes of the listed addresses of the peer. For every
/// direction it keeps the smallest absolute value and the smallest percent value, and a
/// node without a limit wins over every other node. A node that holds no limit of its
/// own takes the limit of the `defnode`.
pub fn session_limits(
    nodes: &[rbinkd_ftn::node::FtnNode],
    addresses: &[rbinkd_ftn::addr::FtnAddr],
) -> ((u64, u64), (u64, u64)) {
    let mut send = (0u64, 0u64);
    let mut recv = (0u64, 0u64);
    let mut send_unlimited = false;
    let mut recv_unlimited = false;
    for fa in addresses {
        let Some(node) = nodes.iter().find(|node| &node.fa == fa) else {
            continue;
        };
        if node.listed == rbinkd_ftn::node::NL_UNLISTED {
            continue;
        }
        if node.bw_send == 0 {
            send_unlimited = true;
        } else if node.bw_send < 0 {
            let percent = node.bw_send.unsigned_abs();
            if send.1 == 0 || percent < send.1 {
                send.1 = percent;
            }
        } else if send.0 == 0 || (node.bw_send as u64) < send.0 {
            send.0 = node.bw_send as u64;
        }
        if node.bw_recv == 0 {
            recv_unlimited = true;
        } else if node.bw_recv < 0 {
            let percent = node.bw_recv.unsigned_abs();
            if recv.1 == 0 || percent < recv.1 {
                recv.1 = percent;
            }
        } else if recv.0 == 0 || (node.bw_recv as u64) < recv.0 {
            recv.0 = node.bw_recv as u64;
        }
    }
    // The `defnode` answers for a direction without a limit of its own. The C code
    // reads the node of the address `defnode`.
    let defnode = nodes.iter().find(|node| node.fa.domain == "defnode");
    if let Some(defnode) = defnode {
        if send_unlimited {
            send = (0, 0);
        } else if send == (0, 0) {
            if defnode.bw_send >= 0 {
                send.0 = defnode.bw_send as u64;
            } else {
                send.1 = defnode.bw_send.unsigned_abs();
            }
        }
        if recv_unlimited {
            recv = (0, 0);
        } else if recv == (0, 0) {
            if defnode.bw_recv >= 0 {
                recv.0 = defnode.bw_recv as u64;
            } else {
                recv.1 = defnode.bw_recv.unsigned_abs();
            }
        }
    }
    (send, recv)
}

/// Returns the limit of a file, the port of `setup_rate_limit()`.
///
/// The rate of a rule is the limit of the file. A positive rate is an absolute
/// value in bytes per second, and the percent of the node scales it. A negative rate
/// is a percent, and it scales the absolute limit of the node. A rate of zero asks
/// for no limit.
pub fn file_limit(rate: i64, abs: u64, rel: u64) -> u64 {
    if rate == 0 {
        return 0;
    }
    if rate > 0 {
        if rel != 0 {
            rate as u64 * rel / 100
        } else {
            rate as u64
        }
    } else if abs != 0 {
        abs * rate.unsigned_abs() / 100
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_limit_of_a_file() {
        // A rate of zero asks for no limit.
        assert_eq!(file_limit(0, 1000, 50), 0);
        // A rule without a node percent holds the rate itself.
        assert_eq!(file_limit(2048, 0, 0), 2048);
        assert_eq!(file_limit(2048, 1000, 0), 2048);
        // The percent of the node scales the rate of the rule.
        assert_eq!(file_limit(2048, 0, 100), 2048);
        assert_eq!(file_limit(2048, 0, 50), 1024);
        // A negative rate is a percent of the absolute limit of the node.
        assert_eq!(file_limit(-50, 4096, 0), 2048);
        assert_eq!(file_limit(-100, 4096, 0), 4096);
        // A node without an absolute limit stays unlimited.
        assert_eq!(file_limit(-50, 0, 0), 0);
    }

    #[test]
    fn the_meter_without_a_limit() {
        let mut bw = Bandwidth::default();
        let start = Instant::now();
        bw.bytes = 100_000;
        assert_eq!(bw.check(start), None);
    }

    #[test]
    fn the_meter_of_a_fast_direction() {
        let mut bw = Bandwidth::new(0, 0);
        bw.limit = 1000;
        let start = Instant::now();
        // The first check starts the meter and answers no wait.
        assert_eq!(bw.check(start), None);
        // One megabyte in one second is over the limit of a thousand bytes.
        bw.bytes = 1_000_000;
        let delay = bw.check(start + Duration::from_secs(1)).unwrap();
        assert!(delay.as_micros() > 0, "{delay:?}");
        // The C code waits the window times the relative overshoot and one
        // millisecond: 1000000 us times the factor of a thousand.
        assert_eq!(
            delay.as_micros(),
            (1_000_000.0 * (1_000_000.0 / 1000.0 - 1.0)) as u128 + 1000
        );
    }

    #[test]
    fn the_meter_of_a_slow_direction() {
        let mut bw = Bandwidth::new(0, 0);
        bw.limit = 1000;
        let start = Instant::now();
        assert_eq!(bw.check(start), None);
        // A hundred bytes in one second is below the limit.
        bw.bytes = 100;
        assert_eq!(bw.check(start + Duration::from_secs(1)), None);
        assert_eq!(bw.cps, 100.0);
    }

    #[test]
    fn the_meter_of_the_same_microsecond() {
        let mut bw = Bandwidth::new(0, 0);
        bw.limit = 1000;
        let start = Instant::now();
        assert_eq!(bw.check(start), None);
        // The bytes arrive in the same instant, and the meter holds the short wait.
        bw.bytes = 2000;
        assert_eq!(bw.check(start), Some(Duration::from_micros(SHORT_WAIT)));
        // A count below the limit answers no wait.
        bw.bytes = 500;
        assert_eq!(bw.check(start), None);
    }
}
