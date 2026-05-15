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
    /// HTTP response status was not 2xx (after the 429 retry, if any).
    Status { code: u16, url: String },
    /// `--auth-header "Name: value"` value couldn't be parsed (no `:`
    /// separating the name from the value, or empty name).
    InvalidAuthHeader(String),
    /// `--auth-query "key=value"` value couldn't be parsed (no `=`
    /// separating the key from the value, or empty key).
    InvalidAuthQuery(String),
    /// `--rate-per-sec <N>` value was non-positive, non-finite, or
    /// outside the accepted range.
    InvalidRate(String),
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
            Self::InvalidAuthHeader(s) => write!(
                f,
                "invalid --auth-header value: expected 'Name: value' form, got {s:?}",
            ),
            Self::InvalidAuthQuery(s) => write!(
                f,
                "invalid --auth-query value: expected 'key=value' form, got {s:?}",
            ),
            Self::InvalidRate(s) => write!(
                f,
                "invalid --rate-per-sec value: expected a positive number, got {s:?}",
            ),
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

/// Parsed `--auth-header "Name: value"` argument. The two halves are
/// stored separately so they can be applied to each request via ureq's
/// header API.
#[derive(Debug, Clone)]
pub struct AuthHeader {
    pub name: String,
    pub value: String,
}

impl AuthHeader {
    /// Parse a `"Name: value"` form. Leading/trailing whitespace on
    /// either half is trimmed. An empty name is an error.
    ///
    /// # Errors
    ///
    /// - [`UrlTemplateError::InvalidAuthHeader`] if `:` is missing or
    ///   the name half is empty.
    pub fn parse(s: &str) -> Result<Self, UrlTemplateError> {
        let (name, value) = s
            .split_once(':')
            .ok_or_else(|| UrlTemplateError::InvalidAuthHeader(s.to_string()))?;
        let name = name.trim().to_string();
        let value = value.trim().to_string();
        if name.is_empty() {
            return Err(UrlTemplateError::InvalidAuthHeader(s.to_string()));
        }
        Ok(Self { name, value })
    }
}

/// Parsed `--auth-query "key=value"` argument.
#[derive(Debug, Clone)]
pub struct AuthQuery {
    pub key: String,
    pub value: String,
}

impl AuthQuery {
    /// Parse a `"key=value"` form. Leading/trailing whitespace on the
    /// key is trimmed; the value is taken verbatim (whitespace inside
    /// query values is preserved, percent-encoded by the caller if
    /// needed). An empty key is an error.
    ///
    /// # Errors
    ///
    /// - [`UrlTemplateError::InvalidAuthQuery`] if `=` is missing or
    ///   the key half is empty.
    pub fn parse(s: &str) -> Result<Self, UrlTemplateError> {
        let (key, value) = s
            .split_once('=')
            .ok_or_else(|| UrlTemplateError::InvalidAuthQuery(s.to_string()))?;
        let key = key.trim().to_string();
        let value = value.to_string();
        if key.is_empty() {
            return Err(UrlTemplateError::InvalidAuthQuery(s.to_string()));
        }
        Ok(Self { key, value })
    }
}

/// Append `auth_query` params as a `?...` / `&...` suffix on `url`.
fn append_auth_query(url: &str, auth_query: &[AuthQuery]) -> String {
    if auth_query.is_empty() {
        return url.to_string();
    }
    let mut out = String::with_capacity(url.len() + 32 * auth_query.len());
    out.push_str(url);
    let separator_for_first = if url.contains('?') { '&' } else { '?' };
    for (i, aq) in auth_query.iter().enumerate() {
        out.push(if i == 0 { separator_for_first } else { '&' });
        out.push_str(&aq.key);
        out.push('=');
        out.push_str(&aq.value);
    }
    out
}

/// Synchronous HTTP fetcher for URL-template tile sources.
///
/// Holds a reusable [`ureq::Agent`] (sharing TCP / TLS pools across
/// requests) and tracks the maximum `Last-Modified` header across all
/// successful responses. The tracked value becomes the `build_timestamp`
/// header field on the produced pack — per PLAN.md § Pack identity, the
/// header records source-data freshness, not build wall-clock.
///
/// Each `fetch()` call passes through a per-host rate limiter (see
/// [`crate::sources::rate_limit`]) so slippypack doesn't trip provider
/// limits or violate the OSM tile usage policy. On HTTP 429 the
/// fetcher honors `Retry-After` and retries once before surfacing
/// `Status`.
pub struct UrlFetcher {
    agent: ureq::Agent,
    /// Maximum `Last-Modified` (seconds since Unix epoch) seen across
    /// successful responses. `0` if no response carried a parseable
    /// `Last-Modified` header.
    max_last_modified: u64,
    /// Headers added to every request (set via `--auth-header`).
    auth_headers: Vec<AuthHeader>,
    /// Query parameters appended to every fetched URL (set via
    /// `--auth-query`).
    auth_query: Vec<AuthQuery>,
    /// Per-host request-rate enforcement.
    rate_limit: crate::sources::rate_limit::HostRateLimiter,
}

impl UrlFetcher {
    #[must_use]
    pub fn new() -> Self {
        let agent = ureq::Agent::config_builder()
            .user_agent(concat!("slippypack/", env!("CARGO_PKG_VERSION")))
            // Inspect status codes ourselves so the 429-with-Retry-After
            // path lives in one place. Default-true would turn every
            // non-2xx into an Err before we got to peek at headers.
            .http_status_as_error(false)
            .build()
            .new_agent();
        Self {
            agent,
            max_last_modified: 0,
            auth_headers: Vec::new(),
            auth_query: Vec::new(),
            rate_limit: crate::sources::rate_limit::HostRateLimiter::new(),
        }
    }

    /// Set the per-request `--auth-header` list. Replaces any previous
    /// list.
    pub fn set_auth_headers(&mut self, headers: Vec<AuthHeader>) {
        self.auth_headers = headers;
    }

    /// Set the URL-appended `--auth-query` list. Replaces any previous
    /// list.
    pub fn set_auth_query(&mut self, query: Vec<AuthQuery>) {
        self.auth_query = query;
    }

    /// Override the per-host default rate. Applies to every host the
    /// fetcher sees for the remainder of its lifetime.
    pub fn set_rate_override(&mut self, rate: crate::sources::rate_limit::RatePerSec) {
        self.rate_limit.set_override(rate);
    }

    /// GET `url` and return the response body bytes. Applies any
    /// configured `--auth-header` headers and `--auth-query` params.
    /// Blocks before issuing the request if the per-host rate limit
    /// would otherwise be violated.
    ///
    /// # Errors
    ///
    /// - [`UrlTemplateError::Transport`] for network / TLS / DNS or
    ///   body-read failures (`ureq::Error` covers both).
    /// - [`UrlTemplateError::Status`] for non-2xx responses (after the
    ///   429-with-`Retry-After` retry, if any).
    pub fn fetch(&mut self, url: &str) -> Result<Vec<u8>, UrlTemplateError> {
        let final_url = append_auth_query(url, &self.auth_query);
        if let Some(host) = crate::sources::rate_limit::extract_host(&final_url) {
            self.rate_limit.acquire(&host);
        }
        let response = self.issue_request(&final_url)?;
        let status = response.status();
        if status.as_u16() == 429 {
            // Honor Retry-After, then retry once. A second 429
            // surfaces as `Status` — the caller's run is wrong-shaped
            // for the source (rate too high for the configured quota)
            // and silently retrying further would just look like a hang.
            let retry_delay = retry_after_from_response(&response);
            std::thread::sleep(retry_delay);
            let retried = self.issue_request(&final_url)?;
            return self.consume_response(retried, final_url);
        }
        self.consume_response(response, final_url)
    }

    fn issue_request(&self, url: &str) -> Result<ureq::http::Response<ureq::Body>, ureq::Error> {
        let mut request = self.agent.get(url);
        for header in &self.auth_headers {
            request = request.header(header.name.as_str(), header.value.as_str());
        }
        request.call()
    }

    fn consume_response(
        &mut self,
        response: ureq::http::Response<ureq::Body>,
        final_url: String,
    ) -> Result<Vec<u8>, UrlTemplateError> {
        let status = response.status();
        if !status.is_success() {
            return Err(UrlTemplateError::Status {
                code: status.as_u16(),
                url: final_url,
            });
        }
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

/// Extract a `Retry-After`-derived sleep duration from a 429 response.
/// Falls back to a polite 5-second default if the header is missing or
/// unparseable — well under any real provider's expected backoff but
/// long enough that we're not hammering on a transient throttle.
fn retry_after_from_response(response: &ureq::http::Response<ureq::Body>) -> std::time::Duration {
    const FALLBACK: std::time::Duration = std::time::Duration::from_secs(5);
    let Some(value) = response.headers().get("retry-after") else {
        return FALLBACK;
    };
    let Ok(s) = value.to_str() else {
        return FALLBACK;
    };
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    crate::sources::rate_limit::parse_retry_after(s, now_unix).unwrap_or(FALLBACK)
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
pub(crate) fn parse_http_date(s: &str) -> Option<u64> {
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
    use super::{
        AuthHeader, AuthQuery, UrlTemplate, UrlTemplateError, append_auth_query, parse_http_date,
    };

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

    #[test]
    fn auth_header_parses_simple_pair() {
        let h = AuthHeader::parse("Authorization: Bearer abc123").unwrap();
        assert_eq!(h.name, "Authorization");
        assert_eq!(h.value, "Bearer abc123");
    }

    #[test]
    fn auth_header_value_preserves_internal_colons() {
        // Bearer tokens / JWTs can contain colons inside the value; only
        // the first colon separates name from value.
        let h = AuthHeader::parse("X-Custom: a:b:c").unwrap();
        assert_eq!(h.name, "X-Custom");
        assert_eq!(h.value, "a:b:c");
    }

    #[test]
    fn auth_header_trims_surrounding_whitespace() {
        let h = AuthHeader::parse("  X-Foo  :  bar  ").unwrap();
        assert_eq!(h.name, "X-Foo");
        assert_eq!(h.value, "bar");
    }

    #[test]
    fn auth_header_rejects_missing_colon() {
        let err = AuthHeader::parse("Authorization Bearer abc").unwrap_err();
        assert!(matches!(err, UrlTemplateError::InvalidAuthHeader(_)));
    }

    #[test]
    fn auth_header_rejects_empty_name() {
        let err = AuthHeader::parse(": value").unwrap_err();
        assert!(matches!(err, UrlTemplateError::InvalidAuthHeader(_)));
    }

    #[test]
    fn auth_query_parses_simple_pair() {
        let q = AuthQuery::parse("key=YOUR_TOKEN").unwrap();
        assert_eq!(q.key, "key");
        assert_eq!(q.value, "YOUR_TOKEN");
    }

    #[test]
    fn auth_query_value_preserves_internal_equals() {
        // Base64-encoded values can contain `=` padding; only the first
        // `=` separates key from value.
        let q = AuthQuery::parse("token=abc==").unwrap();
        assert_eq!(q.key, "token");
        assert_eq!(q.value, "abc==");
    }

    #[test]
    fn auth_query_rejects_missing_equals() {
        let err = AuthQuery::parse("just-key").unwrap_err();
        assert!(matches!(err, UrlTemplateError::InvalidAuthQuery(_)));
    }

    #[test]
    fn auth_query_rejects_empty_key() {
        let err = AuthQuery::parse("=value").unwrap_err();
        assert!(matches!(err, UrlTemplateError::InvalidAuthQuery(_)));
    }

    #[test]
    fn append_auth_query_appends_with_question_mark_when_no_query() {
        let q = vec![AuthQuery::parse("k=v").unwrap()];
        assert_eq!(
            append_auth_query("https://example.com/0/0/0.png", &q),
            "https://example.com/0/0/0.png?k=v",
        );
    }

    #[test]
    fn append_auth_query_appends_with_ampersand_when_query_exists() {
        let q = vec![AuthQuery::parse("k=v").unwrap()];
        assert_eq!(
            append_auth_query("https://example.com/0/0/0.png?style=dark", &q),
            "https://example.com/0/0/0.png?style=dark&k=v",
        );
    }

    #[test]
    fn append_auth_query_chains_multiple() {
        let q = vec![
            AuthQuery::parse("a=1").unwrap(),
            AuthQuery::parse("b=2").unwrap(),
        ];
        assert_eq!(
            append_auth_query("https://example.com/tile", &q),
            "https://example.com/tile?a=1&b=2",
        );
    }

    #[test]
    fn append_auth_query_returns_url_unchanged_when_empty() {
        let url = "https://example.com/tile";
        assert_eq!(append_auth_query(url, &[]), url);
    }
}
