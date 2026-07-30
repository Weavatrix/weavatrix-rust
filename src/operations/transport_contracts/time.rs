use super::Value;

pub(super) fn timestamp_millis(value: &Value) -> Option<u64> {
    if let Some(value) = value.as_u64() {
        return Some(if value < 10_000_000_000 {
            value.saturating_mul(1_000)
        } else {
            value
        });
    }
    parse_rfc3339_millis(value.as_str()?)
}

pub(super) fn parse_rfc3339_millis(value: &str) -> Option<u64> {
    let bytes = value.as_bytes();
    if bytes.len() < 20
        || bytes.get(4) != Some(&b'-')
        || bytes.get(7) != Some(&b'-')
        || !matches!(bytes.get(10), Some(b'T' | b't' | b' '))
        || bytes.get(13) != Some(&b':')
        || bytes.get(16) != Some(&b':')
    {
        return None;
    }
    let year = i64::from(decimal(bytes, 0, 4)?);
    let month = i64::from(decimal(bytes, 5, 7)?);
    let day = i64::from(decimal(bytes, 8, 10)?);
    let hour = i64::from(decimal(bytes, 11, 13)?);
    let minute = i64::from(decimal(bytes, 14, 16)?);
    let second = i64::from(decimal(bytes, 17, 19)?);
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let mut cursor = 19_usize;
    let mut millis = 0_i64;
    if bytes.get(cursor) == Some(&b'.') {
        cursor += 1;
        let start = cursor;
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
            cursor += 1;
        }
        let fraction = &value[start..cursor];
        let padded = format!("{fraction:0<3}");
        millis = padded.get(..3)?.parse().ok()?;
    }
    let offset_seconds = match bytes.get(cursor) {
        Some(b'Z' | b'z') if cursor + 1 == bytes.len() => 0_i64,
        Some(sign @ (b'+' | b'-')) if cursor + 6 == bytes.len() => {
            let offset_hour = i64::from(decimal(bytes, cursor + 1, cursor + 3)?);
            let offset_minute = i64::from(decimal(bytes, cursor + 4, cursor + 6)?);
            if bytes.get(cursor + 3) != Some(&b':') || offset_hour > 23 || offset_minute > 59 {
                return None;
            }
            let offset = offset_hour * 3_600 + offset_minute * 60;
            if *sign == b'+' { offset } else { -offset }
        }
        _ => return None,
    };
    let days = days_from_civil(year, month, day)?;
    let seconds = days
        .checked_mul(86_400)?
        .checked_add(hour * 3_600 + minute * 60 + second)?
        .checked_sub(offset_seconds)?;
    u64::try_from(seconds.checked_mul(1_000)?.checked_add(millis)?).ok()
}

pub(super) fn decimal(bytes: &[u8], start: usize, end: usize) -> Option<u32> {
    bytes
        .get(start..end)?
        .iter()
        .try_fold(0_u32, |value, byte| {
            byte.is_ascii_digit()
                .then(|| value * 10 + u32::from(*byte - b'0'))
        })
}

pub(super) fn days_from_civil(year: i64, month: i64, day: i64) -> Option<i64> {
    let leap = |year: i64| year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let max_day = match month {
        2 if leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => return None,
    };
    if !(1..=max_day).contains(&day) {
        return None;
    }
    let adjusted_year = year - i64::from(month <= 2);
    let era = if adjusted_year >= 0 {
        adjusted_year
    } else {
        adjusted_year - 399
    } / 400;
    let year_of_era = adjusted_year - era * 400;
    let adjusted_month = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * adjusted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some(era * 146_097 + day_of_era - 719_468)
}
