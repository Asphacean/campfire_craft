//! D-08's log file: append-only lines, UTC timestamp + level, written to
//! [`crate::paths::log_path`], rotated once the file passes a couple of
//! megabytes (one generation is enough — this is a diagnostic aid, not an
//! audit trail). Secrets are never written directly: everything that might
//! carry one goes through [`redact`] first.

use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

const ROTATE_AT_BYTES: u64 = 2 * 1024 * 1024;

/// Length + a fixed marker — never the content. Used for every password,
/// token, and refresh-token value that touches a log line (T-04-01-04).
pub fn redact(secret: &str) -> String {
    format!("<redacted, {} bytes>", secret.len())
}

/// A minimal UTC ISO-8601 timestamp (`YYYY-MM-DDThh:mm:ssZ`), computed from
/// `SystemTime` with no calendar-crate dependency — this is the only place
/// in the whole launcher that needs one, and civil-from-days is ~15 lines.
fn utc_now_iso8601() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let days = secs.div_euclid(86_400);
    let time_of_day = secs.rem_euclid(86_400);
    let (hour, minute, second) = (time_of_day / 3600, (time_of_day / 60) % 60, time_of_day % 60);

    // Howard Hinnant's civil_from_days, days since 1970-01-01.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if month <= 2 { y + 1 } else { y };

    format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z"
    )
}

fn rotate_if_needed(path: &std::path::Path) {
    if let Ok(meta) = std::fs::metadata(path) {
        if meta.len() > ROTATE_AT_BYTES {
            let rotated = path.with_extension("log.1");
            let _ = std::fs::rename(path, rotated);
        }
    }
}

/// Append one `[LEVEL] message` line, UTC-timestamped, to `launcher.log`.
/// Never fails loudly: a diagnostic log write that can't happen must not
/// crash the launcher over it.
pub fn log_line(level: &str, message: &str) {
    let path = crate::paths::log_path();
    rotate_if_needed(&path);
    let line = format!("{} [{level}] {message}\n", utc_now_iso8601());
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = file.write_all(line.as_bytes());
    }
}

pub fn info(message: &str) {
    log_line("INFO", message);
}

pub fn error(message: &str) {
    log_line("ERROR", message);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_never_contains_the_secret() {
        let secret = "hunter2-super-secret";
        let redacted = redact(secret);
        assert!(!redacted.contains(secret));
        assert!(redacted.contains("20 bytes"));
    }

    #[test]
    fn timestamp_has_the_expected_shape() {
        let ts = utc_now_iso8601();
        assert_eq!(ts.len(), 20);
        assert!(ts.ends_with('Z'));
    }
}
