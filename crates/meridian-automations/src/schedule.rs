//! Schedule parsing + next-fire computation.
//!
//! Cron support is intentionally minimal — we only honor the five-field syntax
//! and the special common-case patterns the inbox spec template emits
//! (e.g. `0 9 * * *`). Anything more exotic falls back to "fire every minute"
//! semantics and we surface a parse error to the UI rather than panic.

use chrono::{DateTime, Datelike, Duration, NaiveTime, TimeZone, Timelike, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Schedule {
    Cron { cron: String },
    Every { every: EveryShortcut },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum EveryShortcut {
    #[serde(rename = "1h")]
    OneHour,
    #[serde(rename = "6h")]
    SixHours,
    #[serde(rename = "1d")]
    OneDay,
}

impl EveryShortcut {
    pub fn duration(&self) -> Duration {
        match self {
            Self::OneHour => Duration::hours(1),
            Self::SixHours => Duration::hours(6),
            Self::OneDay => Duration::days(1),
        }
    }

    pub fn human(&self) -> &'static str {
        match self {
            Self::OneHour => "every hour",
            Self::SixHours => "every 6 hours",
            Self::OneDay => "every day",
        }
    }
}

impl Schedule {
    pub fn human(&self) -> String {
        match self {
            Self::Every { every } => every.human().to_string(),
            Self::Cron { cron } => format!("cron `{cron}`"),
        }
    }

    /// Compute the next firing time strictly after `from`.
    pub fn next_after(&self, from: DateTime<Utc>) -> DateTime<Utc> {
        match self {
            Self::Every { every } => from + every.duration(),
            Self::Cron { cron } => next_cron(cron, from).unwrap_or_else(|| from + Duration::hours(1)),
        }
    }
}

/// Minimal five-field cron evaluator: `minute hour day-of-month month day-of-week`.
/// Each field accepts `*`, a single integer, or a comma-separated list of
/// integers. Step (`*/N`) and range (`a-b`) syntaxes are not supported. On any
/// parse failure returns None — the caller falls back to a default cadence.
pub fn next_cron(expr: &str, from: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let parts: Vec<&str> = expr.split_whitespace().collect();
    if parts.len() != 5 {
        return None;
    }
    let minute = parse_field(parts[0], 0, 59)?;
    let hour = parse_field(parts[1], 0, 23)?;
    let dom = parse_field(parts[2], 1, 31)?;
    let month = parse_field(parts[3], 1, 12)?;
    let dow = parse_field(parts[4], 0, 6)?; // Sunday=0

    // Walk forward minute-by-minute up to one year. Slow but bounded and
    // dependency-free — for the v1 pattern set (daily/hourly), we'd typically
    // match within a few iterations.
    let mut t = from + Duration::minutes(1);
    t = t.with_second(0)?.with_nanosecond(0)?;
    for _ in 0..(60 * 24 * 366) {
        let m = t.minute();
        let h = t.hour();
        let d = t.day();
        let mo = t.month();
        let wd = t.weekday().num_days_from_sunday();
        if matches(&minute, m)
            && matches(&hour, h)
            && matches(&dom, d)
            && matches(&month, mo)
            && matches(&dow, wd)
        {
            return Some(t);
        }
        t += Duration::minutes(1);
    }
    None
}

#[derive(Clone)]
enum Field {
    Any,
    Set(Vec<u32>),
}

fn parse_field(s: &str, lo: u32, hi: u32) -> Option<Field> {
    if s == "*" {
        return Some(Field::Any);
    }
    let mut out = Vec::new();
    for piece in s.split(',') {
        let n: u32 = piece.parse().ok()?;
        if n < lo || n > hi {
            return None;
        }
        out.push(n);
    }
    Some(Field::Set(out))
}

fn matches(f: &Field, v: u32) -> bool {
    match f {
        Field::Any => true,
        Field::Set(s) => s.contains(&v),
    }
}

/// Backoff policy for failing automations: exponential from 30s up to 1h.
pub fn backoff_for(failure_count: i64) -> Duration {
    let secs = 30u64.saturating_mul(2u64.saturating_pow(failure_count.min(7) as u32));
    Duration::seconds(secs.min(3600) as i64)
}

/// Pick a humane default for newly registered automations: fire on the
/// next minute boundary so the first run is observable but not instant.
pub fn initial_next_run_at(now: DateTime<Utc>) -> DateTime<Utc> {
    let base = now + Duration::minutes(1);
    let nt = NaiveTime::from_hms_opt(base.hour(), base.minute(), 0).unwrap();
    Utc.from_utc_datetime(&base.date_naive().and_time(nt))
}
