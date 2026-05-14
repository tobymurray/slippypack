//! HTTPS URL-template tile source.
//!
//! `--source 'https://.../{z}/{x}/{y}.png'` fetches each tile via HTTP
//! GET against the URL template, substituting `{z}`, `{x}`, `{y}` with
//! the tile coordinate per the slippy-map convention.
//!
//! Per PLAN.md § Source-kind details:
//!
//! > URL templates — direct `https://.../{z}/{x}/{y}.png` URL as the
//! > `--source` value. Authentication is per-source: for a single
//! > source, the CLI flags `--auth-header "Name: value"` and
//! > `--auth-query "key=value"` work (both are repeatable); for
//! > multi-source builds, use `--config slippypack.toml`'s per-source
//! > `auth_header` / `auth_query` fields instead.
//!
//! Phase 1 first slice ships the basic URL fetch (no auth flags yet).
//! Phase 1.x adds `--auth-header` / `--auth-query` and `--config`.

/// Errors that can surface during URL-template substitution or HTTP fetch.
#[derive(Debug)]
#[non_exhaustive]
pub enum UrlTemplateError {
    /// `--source` URL doesn't start with `http://` or `https://`.
    NotHttpScheme,
    /// `--source` URL is missing one or more required placeholders
    /// (`{z}`, `{x}`, `{y}`).
    MissingPlaceholders { missing: Vec<&'static str> },
    /// HTTP request failed (connect timeout, DNS failure, etc).
    Transport(ureq::Error),
    /// HTTP response status was not 2xx.
    Status { code: u16, url: String },
}

impl core::fmt::Display for UrlTemplateError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotHttpScheme => f.write_str("URL must start with http:// or https://"),
            Self::MissingPlaceholders { missing } => {
                write!(
                    f,
                    "URL template is missing required placeholders: {missing:?}"
                )
            }
            Self::Transport(e) => write!(f, "HTTP transport error: {e}"),
            Self::Status { code, url } => {
                write!(f, "HTTP {code} from {url}")
            }
        }
    }
}

impl std::error::Error for UrlTemplateError {}

impl From<ureq::Error> for UrlTemplateError {
    fn from(e: ureq::Error) -> Self {
        Self::Transport(e)
    }
}

/// A parsed URL template with placeholders for `{z}`, `{x}`, `{y}`.
#[derive(Debug, Clone)]
pub struct UrlTemplate {
    template: String,
}

impl UrlTemplate {
    /// Parse and validate a URL template. Returns an error if the URL
    /// doesn't have an HTTP(S) scheme or if any required placeholder is
    /// missing.
    ///
    /// # Errors
    ///
    /// - [`UrlTemplateError::NotHttpScheme`] for non-HTTP(S) URLs.
    /// - [`UrlTemplateError::MissingPlaceholders`] if any of `{z}`,
    ///   `{x}`, `{y}` is missing from the template.
    pub fn parse(s: &str) -> Result<Self, UrlTemplateError> {
        if !s.starts_with("http://") && !s.starts_with("https://") {
            return Err(UrlTemplateError::NotHttpScheme);
        }
        let mut missing: Vec<&'static str> = Vec::new();
        if !s.contains("{z}") {
            missing.push("{z}");
        }
        if !s.contains("{x}") {
            missing.push("{x}");
        }
        if !s.contains("{y}") {
            missing.push("{y}");
        }
        if !missing.is_empty() {
            return Err(UrlTemplateError::MissingPlaceholders { missing });
        }
        Ok(Self {
            template: s.to_string(),
        })
    }

    /// Substitute `{z}`, `{x}`, `{y}` to produce a concrete URL for a tile.
    #[must_use]
    pub fn url_for(&self, z: u8, x: u32, y: u32) -> String {
        self.template
            .replace("{z}", &z.to_string())
            .replace("{x}", &x.to_string())
            .replace("{y}", &y.to_string())
    }
}

/// Synchronous HTTP fetcher for URL-template tile sources.
///
/// Holds a reusable [`ureq::Agent`] (sharing TCP / TLS pools across
/// requests) and tracks the maximum `Last-Modified` header across all
/// successful responses. The tracked value becomes the `build_timestamp`
/// header field on the produced pack — per PLAN.md § Pack identity, the
/// header records source-data freshness, not build wall-clock.
pub struct UrlFetcher {
    agent: ureq::Agent,
    /// Maximum `Last-Modified` (seconds since Unix epoch) seen across
    /// successful responses. `0` if no response carried a parseable
    /// `Last-Modified` header.
    max_last_modified: u64,
}

impl UrlFetcher {
    #[must_use]
    pub fn new() -> Self {
        let agent = ureq::Agent::config_builder()
            .user_agent(concat!("slippypack/", env!("CARGO_PKG_VERSION")))
            .build()
            .new_agent();
        Self {
            agent,
            max_last_modified: 0,
        }
    }

    /// GET `url` and return the response body bytes.
    ///
    /// # Errors
    ///
    /// - [`UrlTemplateError::Transport`] for network / TLS / DNS or
    ///   body-read failures (`ureq::Error` covers both).
    /// - [`UrlTemplateError::Status`] for non-2xx responses.
    pub fn fetch(&mut self, url: &str) -> Result<Vec<u8>, UrlTemplateError> {
        let response = self.agent.get(url).call()?;
        let status = response.status();
        if !status.is_success() {
            return Err(UrlTemplateError::Status {
                code: status.as_u16(),
                url: url.to_string(),
            });
        }
        // Capture Last-Modified before consuming the response.
        if let Some(value) = response.headers().get("last-modified")
            && let Ok(s) = value.to_str()
            && let Some(parsed) = parse_http_date(s)
        {
            self.max_last_modified = self.max_last_modified.max(parsed);
        }
        let bytes = response.into_body().read_to_vec()?;
        Ok(bytes)
    }

    /// Maximum `Last-Modified` (Unix seconds) seen so far. Zero if no
    /// fetched response carried a parseable `Last-Modified` header.
    #[must_use]
    pub fn max_last_modified(&self) -> u64 {
        self.max_last_modified
    }
}

impl Default for UrlFetcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Best-effort parse of an HTTP-date string (RFC 7231) into Unix seconds.
/// Returns `None` for unparseable input.
///
/// Supports the canonical RFC 7231 IMF-fixdate form:
/// `"Sun, 06 Nov 1994 08:49:37 GMT"`. The two obsolete forms (RFC 850 and
/// asctime) are not handled — modern tile servers use IMF-fixdate.
fn parse_http_date(s: &str) -> Option<u64> {
    // Expected layout: "Day, DD Mon YYYY HH:MM:SS GMT"
    // We don't validate the day-of-week (it's redundant with the date).
    let s = s.trim();
    let after_comma = s.split_once(',')?.1.trim_start();
    let mut parts = after_comma.split_whitespace();
    let day: u32 = parts.next()?.parse().ok()?;
    let month = match parts.next()? {
        "Jan" => 1,
        "Feb" => 2,
        "Mar" => 3,
        "Apr" => 4,
        "May" => 5,
        "Jun" => 6,
        "Jul" => 7,
        "Aug" => 8,
        "Sep" => 9,
        "Oct" => 10,
        "Nov" => 11,
        "Dec" => 12,
        _ => return None,
    };
    let year: i32 = parts.next()?.parse().ok()?;
    let time = parts.next()?;
    let mut time_parts = time.split(':');
    let hour: u32 = time_parts.next()?.parse().ok()?;
    let minute: u32 = time_parts.next()?.parse().ok()?;
    let second: u32 = time_parts.next()?.parse().ok()?;
    // Optional "GMT" / "UTC" suffix; we assume UTC regardless (per RFC 7231).
    parts.next();
    days_since_epoch_to_unix(year, month, day, hour, minute, second)
}

/// Compute Unix seconds for `(year, month, day, hour, minute, second)`
/// assuming UTC. Returns `None` for invalid dates or pre-1970 inputs.
fn days_since_epoch_to_unix(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
) -> Option<u64> {
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
        || year < 1970
    {
        return None;
    }
    // Howard Hinnant's days_from_civil algorithm (returns days since 1970-01-01).
    let m_i32 = i32::try_from(month).ok()?; // 1..=12 always fits
    let day_i32 = i32::try_from(day).ok()?; // 1..=31 always fits
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe_signed = y - era * 400; // [0, 399]
    let yoe = u32::try_from(yoe_signed).ok()?;
    let doy_signed = (153 * (m_i32 + (if m_i32 > 2 { -3 } else { 9 })) + 2) / 5 + day_i32 - 1;
    let doy = u32::try_from(doy_signed).ok()?;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    let days = i64::from(era) * 146_097 + i64::from(doe) - 719_468;
    if days < 0 {
        return None;
    }
    let days_u64 = u64::try_from(days).ok()?;
    let total_seconds = days_u64.checked_mul(86_400)?
        + u64::from(hour) * 3_600
        + u64::from(minute) * 60
        + u64::from(second);
    Some(total_seconds)
}

#[cfg(test)]
mod tests {
    use super::{UrlTemplate, UrlTemplateError, parse_http_date};

    #[test]
    fn parse_accepts_https_with_placeholders() {
        let t = UrlTemplate::parse("https://tile.openstreetmap.org/{z}/{x}/{y}.png").unwrap();
        assert_eq!(
            t.url_for(10, 511, 340),
            "https://tile.openstreetmap.org/10/511/340.png",
        );
    }

    #[test]
    fn parse_accepts_http() {
        let t = UrlTemplate::parse("http://localhost:8000/{z}/{x}/{y}.png").unwrap();
        assert_eq!(t.url_for(0, 0, 0), "http://localhost:8000/0/0/0.png");
    }

    #[test]
    fn parse_rejects_non_http_scheme() {
        let err = UrlTemplate::parse("file:///path/{z}/{x}/{y}.png").unwrap_err();
        assert!(matches!(err, UrlTemplateError::NotHttpScheme));
    }

    #[test]
    fn parse_rejects_missing_z_placeholder() {
        let err = UrlTemplate::parse("https://example.com/{x}/{y}.png").unwrap_err();
        let UrlTemplateError::MissingPlaceholders { missing } = err else {
            panic!("expected MissingPlaceholders");
        };
        assert_eq!(missing, vec!["{z}"]);
    }

    #[test]
    fn parse_rejects_missing_all_placeholders() {
        let err = UrlTemplate::parse("https://example.com/tile.png").unwrap_err();
        let UrlTemplateError::MissingPlaceholders { missing } = err else {
            panic!("expected MissingPlaceholders");
        };
        assert_eq!(missing.len(), 3);
    }

    #[test]
    fn substitution_replaces_all_three_placeholders() {
        let t = UrlTemplate::parse("https://example.com/some/path/{z}-{x}-{y}/tile.jpg").unwrap();
        assert_eq!(
            t.url_for(17, 65432, 12345),
            "https://example.com/some/path/17-65432-12345/tile.jpg",
        );
    }

    #[test]
    fn substitution_handles_repeated_placeholders() {
        // If a template repeats a placeholder, all occurrences get substituted.
        let t = UrlTemplate::parse("https://example.com/{z}/{z}/{x}/{y}.png").unwrap();
        assert_eq!(t.url_for(5, 10, 20), "https://example.com/5/5/10/20.png",);
    }

    #[test]
    fn parse_http_date_imf_fixdate() {
        // Standard RFC 7231 IMF-fixdate (matches the example date in the
        // RFC's prose).
        let parsed = parse_http_date("Sun, 06 Nov 1994 08:49:37 GMT").unwrap();
        // 1994-11-06 08:49:37 UTC = 784111777 Unix seconds:
        //   - 9075 days since 1970-01-01 → 9075 * 86400 = 784_080_000
        //   - + 8 * 3600 + 49 * 60 + 37 = 31_777
        //   - total = 784_111_777
        assert_eq!(parsed, 784_111_777);
    }

    #[test]
    fn parse_http_date_epoch() {
        let parsed = parse_http_date("Thu, 01 Jan 1970 00:00:00 GMT").unwrap();
        assert_eq!(parsed, 0);
    }

    #[test]
    fn parse_http_date_modern_value() {
        // 2024-01-01 00:00:00 UTC = 1704067200.
        let parsed = parse_http_date("Mon, 01 Jan 2024 00:00:00 GMT").unwrap();
        assert_eq!(parsed, 1_704_067_200);
    }

    #[test]
    fn parse_http_date_rejects_malformed() {
        assert!(parse_http_date("not a date").is_none());
        assert!(parse_http_date("Sun, 06 Foo 1994 08:49:37 GMT").is_none());
        // Pre-1970 dates aren't representable as u64-seconds-since-epoch.
        assert!(parse_http_date("Tue, 06 Nov 1900 08:49:37 GMT").is_none());
    }
}
