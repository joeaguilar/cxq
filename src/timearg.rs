use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};

pub fn parse(value: &str, end_of_day: bool) -> Result<DateTime<Utc>, String> {
    if let Some(relative) = parse_relative(value) {
        return Ok(Utc::now() - relative);
    }
    if let Ok(ts) = DateTime::parse_from_rfc3339(value) {
        return Ok(ts.with_timezone(&Utc));
    }
    if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        let time = if end_of_day {
            date.and_hms_opt(23, 59, 59)
        } else {
            date.and_hms_opt(0, 0, 0)
        }
        .expect("valid clock time");
        return Ok(Utc.from_utc_datetime(&time));
    }
    Err(format!(
        "invalid time '{value}' (use RFC3339, YYYY-MM-DD, or 7d/24h/90m)"
    ))
}

fn parse_relative(value: &str) -> Option<Duration> {
    let split = value.len().checked_sub(1)?;
    let (number, unit) = value.split_at(split);
    let amount = number.parse::<i64>().ok()?;
    match unit {
        "d" => Some(Duration::days(amount)),
        "h" => Some(Duration::hours(amount)),
        "m" => Some(Duration::minutes(amount)),
        _ => None,
    }
}

pub fn event(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|ts| ts.with_timezone(&Utc))
}
