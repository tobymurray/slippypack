//! Per-host request rate limiter for HTTP tile sources.
//!
//! Enforces a minimum interval between requests to the same host so
//! slippypack doesn't trip provider rate-limits or violate usage
//! policies. Single-threaded sleep-based: the CLI's fetch loop is
//! sequential today, so a `std::thread::sleep` is enough to space
//! requests. A multi-worker variant would need an `Arc<Mutex<...>>`
//! wrapper, but that's deferred until the fetch loop itself goes
//! parallel.
//!
//! ## Defaults
//!
//! The built-in table currently special-cases the OSM standard tile
//! server. Everything else gets a conservative "unknown host" default.
//! Both can be overridden by `--rate-per-sec <N>` on the CLI; the
//! override applies to every host fetched in that run.
//!
//! | Host pattern                       | Default rate |
//! |------------------------------------|--------------|
//! | `*.tile.openstreetmap.org`         | 2 req/sec    |
//! | `tile.openstreetmap.org`           | 2 req/sec    |
//! | (anything else)                    | 4 req/sec    |
//!
//! OSM's published tile usage policy (operations.osmfoundation.org)
//! caps heavy users at "no more than 2 download threads" and expects
//! reasonable rates; 2 req/sec keeps a single-threaded fetcher
//! comfortably inside that envelope. The unknown-host default is
//! deliberately polite — users with a paid tile-source quota will
//! typically want to raise it via `--rate-per-sec`.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// A request rate expressed in tiles per second. Stored as the
/// minimum interval between requests (its reciprocal) so the
/// hot-path arithmetic in [`compute_delay`] stays integer-friendly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RatePerSec {
    min_interval: Duration,
}

impl RatePerSec {
    /// OSM standard tile server default (2 req/sec).
    pub const OSM: Self = Self {
        min_interval: Duration::from_millis(500),
    };

    /// Default for hosts not in the built-in table (4 req/sec).
    pub const UNKNOWN_DEFAULT: Self = Self {
        min_interval: Duration::from_millis(250),
    };

    /// Construct from a positive `req_per_sec` rate. Returns `None` if
    /// the input is non-positive or non-finite. CLI override path —
    /// the consts above cover the built-in defaults.
    #[must_use]
    pub fn from_req_per_sec(req_per_sec: f64) -> Option<Self> {
        if !req_per_sec.is_finite() || req_per_sec <= 0.0 {
            return None;
        }
        let seconds = 1.0 / req_per_sec;
        Some(Self {
            min_interval: Duration::from_secs_f64(seconds),
        })
    }

    /// Minimum interval between consecutive requests to a host
    /// governed by this rate.
    #[must_use]
    pub fn min_interval(self) -> Duration {
        self.min_interval
    }
}

/// Per-host rate-limit tracker. Holds the last-issued time per host;
/// `acquire(host)` blocks until the next request is safe.
pub struct HostRateLimiter {
    override_rate: Option<RatePerSec>,
    last_seen: HashMap<String, Instant>,
}

impl HostRateLimiter {
    #[must_use]
    pub fn new() -> Self {
        Self {
            override_rate: None,
            last_seen: HashMap::new(),
        }
    }

    /// Set a rate that supersedes the built-in per-host defaults.
    /// Applies to every host this limiter sees from now on.
    pub fn set_override(&mut self, rate: RatePerSec) {
        self.override_rate = Some(rate);
    }

    /// Resolve the per-host rate: override wins, else the built-in
    /// table, else the unknown-host default.
    fn rate_for(&self, host: &str) -> RatePerSec {
        if let Some(rate) = self.override_rate {
            return rate;
        }
        if is_osm_host(host) {
            RatePerSec::OSM
        } else {
            RatePerSec::UNKNOWN_DEFAULT
        }
    }

    /// Block (via `std::thread::sleep`) until issuing a request to
    /// `host` is safe under the active rate, then record that a
    /// request is being issued now.
    pub fn acquire(&mut self, host: &str) {
        let host_lc = host.to_ascii_lowercase();
        let rate = self.rate_for(&host_lc);
        let now = Instant::now();
        let last = self.last_seen.get(&host_lc).copied();
        let delay = compute_delay(last, now, rate.min_interval());
        if !delay.is_zero() {
            std::thread::sleep(delay);
        }
        self.last_seen.insert(host_lc, Instant::now());
    }
}

impl Default for HostRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

/// Pure helper: how long must a request wait before being safe?
/// Returns `Duration::ZERO` if no wait is needed (first request, or
/// enough time has elapsed since the last one).
fn compute_delay(last_seen: Option<Instant>, now: Instant, interval: Duration) -> Duration {
    match last_seen {
        Some(prev) => {
            let elapsed = now.saturating_duration_since(prev);
            interval.saturating_sub(elapsed)
        }
        None => Duration::ZERO,
    }
}

/// `true` for hosts served by the OSM standard tile server. Matches
/// `tile.openstreetmap.org` exactly and any subdomain ending in
/// `.tile.openstreetmap.org` (covers `a.`/`b.`/`c.` subdomains).
fn is_osm_host(host_lc: &str) -> bool {
    host_lc == "tile.openstreetmap.org" || host_lc.ends_with(".tile.openstreetmap.org")
}

/// Extract the host portion of an HTTP(S) URL. Handles `user:pass@`
/// userinfo and `:port` suffix; returns `None` if the URL has no
/// `://` separator. Result is lowercased.
#[must_use]
pub fn extract_host(url: &str) -> Option<String> {
    let (_, rest) = url.split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next()?;
    if authority.is_empty() {
        return None;
    }
    // Strip optional userinfo (everything up to and including the
    // last '@' before the host).
    let after_userinfo = match authority.rsplit_once('@') {
        Some((_, host_part)) => host_part,
        None => authority,
    };
    // Strip optional :port. Bracketed IPv6 literals must not be split on the
    // colons inside the brackets, so they are unwrapped first: `[::1]:8080` → `::1`.
    // Without this the host key for any IPv6 URL is garbage, which breaks per-host
    // pacing and makes `::1` unrecognisable as loopback.
    let host = if let Some(rest) = after_userinfo.strip_prefix('[') {
        // An unterminated bracket is malformed, not a host.
        rest.split_once(']')?.0
    } else {
        match after_userinfo.rsplit_once(':') {
            Some((h, _port)) => h,
            None => after_userinfo,
        }
    };
    if host.is_empty() {
        None
    } else {
        Some(host.to_ascii_lowercase())
    }
}

/// Parse an HTTP `Retry-After` header value. RFC 9110 § 10.2.3
/// permits either a non-negative integer (delta-seconds) or an
/// HTTP-date. Returns `None` for unparseable input.
#[must_use]
pub fn parse_retry_after(value: &str, now_unix: u64) -> Option<Duration> {
    let trimmed = value.trim();
    if let Ok(secs) = trimmed.parse::<u64>() {
        return Some(Duration::from_secs(secs));
    }
    // HTTP-date form. Reuse url_template's HTTP-date parser by
    // duplicating the parse here — we only need delta from `now_unix`.
    let target = crate::sources::url_template::parse_http_date(trimmed)?;
    Some(Duration::from_secs(target.saturating_sub(now_unix)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osm_host_matchers() {
        assert!(is_osm_host("tile.openstreetmap.org"));
        assert!(is_osm_host("a.tile.openstreetmap.org"));
        assert!(is_osm_host("c.tile.openstreetmap.org"));
        assert!(!is_osm_host("openstreetmap.org"));
        assert!(!is_osm_host("tile.openstreetmap.org.evil.example"));
        assert!(!is_osm_host("api.maptiler.com"));
    }

    #[test]
    fn extract_host_strips_scheme_and_path() {
        assert_eq!(
            extract_host("https://a.tile.openstreetmap.org/3/4/5.png").as_deref(),
            Some("a.tile.openstreetmap.org"),
        );
        assert_eq!(
            extract_host("http://example.com/").as_deref(),
            Some("example.com"),
        );
    }

    #[test]
    fn extract_host_strips_port() {
        assert_eq!(
            extract_host("http://example.com:8080/path").as_deref(),
            Some("example.com"),
        );
    }

    #[test]
    fn extract_host_strips_userinfo() {
        assert_eq!(
            extract_host("https://user:pass@example.com/x").as_deref(),
            Some("example.com"),
        );
    }

    #[test]
    fn extract_host_lowercases() {
        assert_eq!(
            extract_host("https://Tile.OpenStreetMap.ORG/3/4/5.png").as_deref(),
            Some("tile.openstreetmap.org"),
        );
    }

    #[test]
    fn extract_host_handles_query_and_fragment() {
        assert_eq!(
            extract_host("https://example.com?key=val").as_deref(),
            Some("example.com"),
        );
        assert_eq!(
            extract_host("https://example.com#frag").as_deref(),
            Some("example.com"),
        );
    }

    #[test]
    fn extract_host_rejects_unparseable() {
        assert_eq!(extract_host("not-a-url"), None);
        assert_eq!(extract_host("https:///no-host"), None);
    }

    #[test]
    fn extract_host_unwraps_bracketed_ipv6() {
        assert_eq!(
            extract_host("http://[::1]:8080/3/4/5.png").as_deref(),
            Some("::1"),
        );
        assert_eq!(extract_host("http://[::1]/x").as_deref(), Some("::1"));
        assert_eq!(
            extract_host("https://[2001:DB8::1]:443/tile").as_deref(),
            Some("2001:db8::1"),
        );
    }

    #[test]
    fn extract_host_rejects_unterminated_bracket() {
        assert_eq!(extract_host("http://[::1/x"), None);
    }

    #[test]
    fn rate_per_sec_from_req_per_sec_rejects_bad_input() {
        assert_eq!(RatePerSec::from_req_per_sec(0.0), None);
        assert_eq!(RatePerSec::from_req_per_sec(-1.0), None);
        assert_eq!(RatePerSec::from_req_per_sec(f64::NAN), None);
        assert_eq!(RatePerSec::from_req_per_sec(f64::INFINITY), None);
    }

    #[test]
    fn rate_per_sec_round_trip() {
        let rate = RatePerSec::from_req_per_sec(10.0).unwrap();
        assert_eq!(rate.min_interval(), Duration::from_millis(100));
    }

    #[test]
    fn osm_default_is_2_per_sec() {
        assert_eq!(RatePerSec::OSM.min_interval(), Duration::from_millis(500));
    }

    #[test]
    fn unknown_default_is_4_per_sec() {
        assert_eq!(
            RatePerSec::UNKNOWN_DEFAULT.min_interval(),
            Duration::from_millis(250),
        );
    }

    #[test]
    fn compute_delay_first_call_is_zero() {
        let now = Instant::now();
        assert_eq!(
            compute_delay(None, now, Duration::from_millis(500)),
            Duration::ZERO,
        );
    }

    #[test]
    fn compute_delay_returns_remaining_when_interval_not_elapsed() {
        let base = Instant::now();
        let interval = Duration::from_millis(500);
        let now = base + Duration::from_millis(150);
        let delay = compute_delay(Some(base), now, interval);
        assert_eq!(delay, Duration::from_millis(350));
    }

    #[test]
    fn compute_delay_returns_zero_when_interval_already_elapsed() {
        let base = Instant::now();
        let now = base + Duration::from_millis(750);
        let delay = compute_delay(Some(base), now, Duration::from_millis(500));
        assert_eq!(delay, Duration::ZERO);
    }

    #[test]
    fn rate_for_picks_osm_default_for_osm_host() {
        let limiter = HostRateLimiter::new();
        assert_eq!(
            limiter.rate_for("a.tile.openstreetmap.org"),
            RatePerSec::OSM,
        );
    }

    #[test]
    fn rate_for_picks_unknown_default_for_other_host() {
        let limiter = HostRateLimiter::new();
        assert_eq!(
            limiter.rate_for("api.maptiler.com"),
            RatePerSec::UNKNOWN_DEFAULT,
        );
    }

    #[test]
    fn override_supersedes_per_host_defaults() {
        let mut limiter = HostRateLimiter::new();
        let override_rate = RatePerSec::from_req_per_sec(20.0).unwrap();
        limiter.set_override(override_rate);
        assert_eq!(limiter.rate_for("a.tile.openstreetmap.org"), override_rate);
        assert_eq!(limiter.rate_for("api.maptiler.com"), override_rate);
    }

    #[test]
    fn acquire_first_call_is_fast() {
        let mut limiter = HostRateLimiter::new();
        limiter.set_override(RatePerSec::from_req_per_sec(10.0).unwrap());
        let start = Instant::now();
        limiter.acquire("example.com");
        assert!(start.elapsed() < Duration::from_millis(50));
    }

    #[test]
    fn acquire_second_call_sleeps_at_least_min_interval() {
        let mut limiter = HostRateLimiter::new();
        // 50 req/sec → 20 ms interval. Pick something short for fast tests.
        limiter.set_override(RatePerSec::from_req_per_sec(50.0).unwrap());
        limiter.acquire("example.com");
        let start = Instant::now();
        limiter.acquire("example.com");
        let elapsed = start.elapsed();
        assert!(
            elapsed >= Duration::from_millis(15),
            "second acquire returned after {elapsed:?}, expected ≥ ~20 ms",
        );
    }

    #[test]
    fn acquire_tracks_hosts_independently() {
        let mut limiter = HostRateLimiter::new();
        limiter.set_override(RatePerSec::from_req_per_sec(50.0).unwrap());
        limiter.acquire("first.example");
        let start = Instant::now();
        limiter.acquire("second.example");
        // Different host → no rate-limit interaction.
        assert!(start.elapsed() < Duration::from_millis(10));
    }

    #[test]
    fn acquire_is_case_insensitive_for_host() {
        let mut limiter = HostRateLimiter::new();
        limiter.set_override(RatePerSec::from_req_per_sec(50.0).unwrap());
        limiter.acquire("Example.COM");
        let start = Instant::now();
        limiter.acquire("example.com");
        assert!(start.elapsed() >= Duration::from_millis(15));
    }

    #[test]
    fn parse_retry_after_seconds_form() {
        assert_eq!(
            parse_retry_after("30", 0),
            Some(Duration::from_secs(30)),
        );
        assert_eq!(
            parse_retry_after("  5  ", 0),
            Some(Duration::from_secs(5)),
        );
    }

    #[test]
    fn parse_retry_after_http_date_form() {
        // Epoch+60 — depends on the http-date parser in url_template.
        // Just verify both well-known forms parse to a sensible delta.
        let result = parse_retry_after("Thu, 01 Jan 1970 00:01:00 GMT", 0);
        assert_eq!(result, Some(Duration::from_mins(1)));
    }

    #[test]
    fn parse_retry_after_past_date_returns_zero() {
        // Server clock skew shouldn't cause negative sleeps.
        let result = parse_retry_after("Thu, 01 Jan 1970 00:00:00 GMT", 1000);
        assert_eq!(result, Some(Duration::ZERO));
    }

    #[test]
    fn parse_retry_after_rejects_garbage() {
        assert_eq!(parse_retry_after("not a date", 0), None);
        assert_eq!(parse_retry_after("", 0), None);
    }
}
