//! Date helpers.
//!
//! The browser reference works with JavaScript `Date` objects. All panorama
//! dates that matter for filtering are month based, so this module exposes
//! month indexed helpers instead of dragging a timezone aware datetime type
//! through the filters.

use chrono::{Datelike, Local, NaiveDate};

pub fn current_year() -> i32 {
    Local::now().year()
}

pub fn current_month() -> String {
    let now = Local::now();
    format!("{:04}-{:02}", now.year(), now.month())
}

/// Parses `YYYY-MM` or any longer ISO-ish date into `(year, month)`.
pub fn parse_month(s: &str) -> Option<(i32, u32)> {
    let s = s.trim();
    let bytes = s.as_bytes();
    if bytes.len() < 7 {
        return None;
    }
    let year: i32 = s.get(0..4)?.parse().ok()?;
    if bytes[4] != b'-' {
        return None;
    }
    let month: u32 = s.get(5..7)?.parse().ok()?;
    if !(1..=12).contains(&month) {
        return None;
    }
    Some((year, month))
}

/// Month index used for cheap comparisons (`year * 12 + (month - 1)`).
pub fn month_index(s: &str) -> Option<i64> {
    let (y, m) = parse_month(s)?;
    Some(y as i64 * 12 + m as i64 - 1)
}

fn month_start(y: i32, m: u32) -> Option<i64> {
    let date = NaiveDate::from_ymd_opt(y, m, 1)?;
    Some(date.and_hms_opt(0, 0, 0)?.and_utc().timestamp_millis())
}

fn month_end(y: i32, m: u32) -> Option<i64> {
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    Some(month_start(ny, nm)? - 1)
}

/// Start of the month in milliseconds, mirroring `Date.parse("YYYY-MM")`.
pub fn month_start_ms(s: &str) -> Option<i64> {
    let (y, m) = parse_month(s)?;
    month_start(y, m)
}

/// End of the month in milliseconds, mirroring the reference `getMonthEndTimestamp`.
pub fn month_end_ms(s: &str) -> Option<i64> {
    let (y, m) = parse_month(s)?;
    month_end(y, m)
}

/// Parses `YYYY`, `YYYY-MM`, `YYYY-MM-DD` and `YYYY-MM-DDTHH:MM[:SS[.sss]]`
/// (with or without a trailing `Z`) into milliseconds since the Unix epoch.
pub fn parse_flexible_ms(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("undefined") {
        return None;
    }
    let bytes = s.as_bytes();
    if bytes.len() < 4 {
        return None;
    }
    let year: i32 = s.get(0..4)?.parse().ok()?;
    let month: u32 = if bytes.len() >= 7 && bytes[4] == b'-' {
        s.get(5..7)?.parse().ok()?
    } else {
        1
    };
    let day: u32 = if bytes.len() >= 10 && bytes[7] == b'-' {
        s.get(8..10)?.parse().ok()?
    } else {
        1
    };
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let (mut hh, mut mm, mut ss) = (0u32, 0u32, 0u32);
    if bytes.len() >= 16 && (bytes[10] == b'T' || bytes[10] == b' ') {
        hh = s.get(11..13)?.parse().ok()?;
        mm = s.get(14..16)?.parse().ok()?;
        if bytes.len() >= 19 && bytes[16] == b':' {
            ss = s.get(17..19)?.parse().ok()?;
        }
    }
    let date = NaiveDate::from_ymd_opt(year, month, day)?;
    let dt = date.and_hms_opt(hh.min(23), mm.min(59), ss.min(59))?;
    Some(dt.and_utc().timestamp_millis())
}

/// Parses Bing's `M/D/YYYY h:mm:ss AM/PM` capture dates into ISO without a zone.
pub fn parse_us_datetime(s: &str) -> Option<String> {
    let s = s.trim();
    let (date_part, time_part) = s.split_once(' ')?;
    let mut date_bits = date_part.split('/');
    let month: u32 = date_bits.next()?.parse().ok()?;
    let day: u32 = date_bits.next()?.parse().ok()?;
    let year: i32 = date_bits.next()?.parse().ok()?;
    if date_bits.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let (clock, meridiem) = match time_part.split_once(' ') {
        Some((clock, meridiem)) => (clock, Some(meridiem.to_ascii_uppercase())),
        None => (time_part, None),
    };
    let mut clock_bits = clock.split(':');
    let mut hour: u32 = clock_bits.next()?.parse().ok()?;
    let minute: u32 = clock_bits.next().unwrap_or("0").parse().ok()?;
    let second: u32 = clock_bits.next().unwrap_or("0").parse().ok()?;
    if let Some(meridiem) = meridiem {
        hour %= 12;
        if meridiem == "PM" {
            hour += 12;
        }
    }
    NaiveDate::from_ymd_opt(year, month, day)?;
    Some(format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{minute:02}:{second:02}",
        hour.min(23)
    ))
}

/// `(year, month)` for the select-months filter, tolerating ISO strings.
pub fn parse_month_year(s: &str) -> Option<(i32, u32)> {
    parse_month(s)
}

/// Tencent and Baidu encode the capture date at the start of their pano IDs:
/// `YYMMDDHHMMSS`.
pub fn extract_date_from_pano_id(pano: &str) -> Option<String> {
    if pano.len() < 12 || !pano.is_char_boundary(12) || !pano[..12].bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let year: i32 = 2000 + pano[0..2].parse::<i32>().ok()?;
    let month: u32 = pano[2..4].parse().ok()?;
    let day: u32 = pano[4..6].parse().ok()?;
    let hour: u32 = pano[6..8].parse().ok()?;
    let minute: u32 = pano[8..10].parse().ok()?;
    let second: u32 = pano[10..12].parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        year, month, day, hour.min(23), minute.min(59), second.min(59)
    ))
}

/// Formats milliseconds since the epoch as a timezone independent ISO string.
pub fn epoch_ms_to_iso(ms: i64) -> String {
    use chrono::{TimeZone, Utc};
    match Utc.timestamp_millis_opt(ms) {
        chrono::LocalResult::Single(dt) | chrono::LocalResult::Ambiguous(dt, _) => {
            dt.format("%Y-%m-%dT%H:%M:%S").to_string()
        }
        chrono::LocalResult::None => String::new(),
    }
}

pub fn epoch_seconds_to_iso(seconds: f64) -> String {
    if !seconds.is_finite() {
        return String::new();
    }
    epoch_ms_to_iso((seconds * 1000.0).round() as i64)
}

/// Normalizes a provider date to the `YYYY-MM` form used for month filters.
pub fn month_of(s: &str) -> Option<String> {
    let (y, m) = parse_month(s)?;
    Some(format!("{:04}-{:02}", y, m))
}

/// `YYMMDD`-style suffix used by Yandex pano IDs (seconds since the epoch).
pub fn yandex_date_from_id(id: &str) -> Option<String> {
    let suffix = id.rsplit('_').next()?;
    let seconds: f64 = suffix.parse().ok()?;
    if !(0.0..=4_102_444_800.0).contains(&seconds) {
        return None;
    }
    Some(epoch_seconds_to_iso(seconds))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_months() {
        assert_eq!(parse_month("2021-10"), Some((2021, 10)));
        assert_eq!(parse_month("2021-10-05T12:00:00"), Some((2021, 10)));
        assert_eq!(parse_month("nope"), None);
        assert_eq!(month_index("2021-01"), Some(2021 * 12));
    }

    #[test]
    fn month_bounds() {
        let start = month_start_ms("2024-02").unwrap();
        let end = month_end_ms("2024-02").unwrap();
        assert_eq!(end - start, 29 * 24 * 3600 * 1000 - 1);
    }

    #[test]
    fn flexible_dates() {
        assert_eq!(parse_flexible_ms("2024-06"), month_start_ms("2024-06"));
        assert!(parse_flexible_ms("2024-06-15T10:20:30Z").is_some());
        assert_eq!(parse_flexible_ms("garbage"), None);
    }

    #[test]
    fn pano_id_dates() {
        assert_eq!(
            extract_date_from_pano_id("230615102030"),
            Some("2023-06-15T10:20:30".to_string())
        );
        assert_eq!(extract_date_from_pano_id("not-a-date"), None);
    }

    #[test]
    fn bing_us_dates() {
        assert_eq!(
            parse_us_datetime("4/24/2021 3:04:31 PM"),
            Some("2021-04-24T15:04:31".to_string())
        );
        assert_eq!(
            parse_us_datetime("12/1/2019 12:05:00 AM"),
            Some("2019-12-01T00:05:00".to_string())
        );
        assert_eq!(parse_us_datetime("nonsense"), None);
    }
}
