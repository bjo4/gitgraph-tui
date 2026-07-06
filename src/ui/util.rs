//! Pure formatting helpers for the UI layer.
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Compact relative age, Git-Graph style.
pub fn relative_time(ts: i64, now: i64) -> String {
    let d = (now - ts).max(0);
    const MIN: i64 = 60;
    const HOUR: i64 = 3_600;
    const DAY: i64 = 86_400;
    const MONTH: i64 = 30 * DAY;
    const YEAR: i64 = 365 * DAY;
    match d {
        _ if d < MIN => "now".to_string(),
        _ if d < HOUR => format!("{}m", d / MIN),
        _ if d < DAY => format!("{}h", d / HOUR),
        _ if d < MONTH => format!("{}d", d / DAY),
        _ if d < YEAR => format!("{}mo", d / MONTH),
        _ => format!("{}y", d / YEAR),
    }
}

/// "YYYY-MM-DD HH:MM" in UTC, for the detail panel.
pub fn absolute_time(ts: i64) -> String {
    use time::macros::format_description;
    let Ok(dt) = time::OffsetDateTime::from_unix_timestamp(ts) else {
        return "?".to_string();
    };
    let fmt = format_description!("[year]-[month]-[day] [hour]:[minute]");
    dt.format(&fmt).unwrap_or_else(|_| "?".to_string())
}

/// Truncate to a display width (CJK-aware); appends '…' when cut.
pub fn truncate_width(s: &str, max: usize) -> String {
    if s.width() <= max {
        return s.to_string();
    }
    let budget = max.saturating_sub(1);
    let mut out = String::new();
    let mut used = 0;
    for ch in s.chars() {
        let w = ch.width().unwrap_or(0);
        if used + w > budget {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push('…');
    out
}

/// Right-pad with spaces to an exact display width (input must already fit).
pub fn pad_to_width(s: &str, width: usize) -> String {
    let pad = width.saturating_sub(s.width());
    format!("{s}{}", " ".repeat(pad))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_time_buckets() {
        let now = 1_000_000_000;
        assert_eq!(relative_time(now - 5, now), "now");
        assert_eq!(relative_time(now - 300, now), "5m");
        assert_eq!(relative_time(now - 7_200, now), "2h");
        assert_eq!(relative_time(now - 3 * 86_400, now), "3d");
        assert_eq!(relative_time(now - 65 * 86_400, now), "2mo");
        assert_eq!(relative_time(now - 800 * 86_400, now), "2y");
        assert_eq!(relative_time(now + 999, now), "now"); // clock skew clamps
    }

    #[test]
    fn absolute_time_formats_utc() {
        assert_eq!(absolute_time(0), "1970-01-01 00:00");
    }

    #[test]
    fn truncate_respects_cjk_double_width() {
        assert_eq!(truncate_width("hello", 10), "hello");
        assert_eq!(truncate_width("hello world", 8), "hello w…");
        // Each CJK char is width 2: "訊息" = 4 columns.
        assert_eq!(truncate_width("訊息訊息", 5), "訊息…");
    }

    #[test]
    fn pad_fills_to_exact_width() {
        assert_eq!(pad_to_width("ab", 4), "ab  ");
        assert_eq!(pad_to_width("訊息", 6), "訊息  ");
    }
}
