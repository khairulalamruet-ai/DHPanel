//! When backups are made (T09.07): a five-field cron expression in UTC, and which backups
//! retention keeps. Pure functions of numbers, no clock and no database.

use thiserror::Error;

/// Why a cron expression is not usable.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CronError {
    /// Not five fields.
    #[error("a cron expression has five fields: minute hour day-of-month month day-of-week")]
    Fields,
    /// A field that cannot be read or is out of range.
    #[error("the {0} field is not valid")]
    Field(&'static str),
}

/// A parsed cron expression: minute, hour, day of month, month, day of week (0 or 7 is
/// Sunday). `*`, `a-b`, `*/n`, `a-b/n` and comma lists. When both the day of month and
/// the day of week are restricted a day matches if either does (as cron does).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cron {
    minutes: u64,
    hours: u32,
    days: u32,
    months: u16,
    weekdays: u8,
    any_day: bool,
    any_weekday: bool,
}

fn field(text: &str, name: &'static str, min: u32, max: u32) -> Result<(u64, bool), CronError> {
    let bad = || CronError::Field(name);
    let mut bits = 0u64;
    let mut star = false;
    for item in text.split(',') {
        let (range, step) = match item.split_once('/') {
            Some((r, s)) => (r, s.parse::<u32>().map_err(|_| bad())?),
            None => (item, 1),
        };
        if step == 0 {
            return Err(bad());
        }
        let (from, to) = if range == "*" {
            star = star || item == "*";
            (min, max)
        } else if let Some((a, b)) = range.split_once('-') {
            (
                a.parse::<u32>().map_err(|_| bad())?,
                b.parse::<u32>().map_err(|_| bad())?,
            )
        } else {
            let v = range.parse::<u32>().map_err(|_| bad())?;
            (v, if item.contains('/') { max } else { v })
        };
        if from < min || to > max || from > to {
            return Err(bad());
        }
        let mut v = from;
        while v <= to {
            bits |= 1 << v;
            v += step;
        }
    }
    Ok((bits, star))
}

impl Cron {
    /// Reads an expression.
    ///
    /// # Errors
    /// [`CronError`].
    pub fn parse(text: &str) -> Result<Self, CronError> {
        let fields: Vec<&str> = text.split_whitespace().collect();
        let [m, h, dom, mon, dow] = fields[..] else {
            return Err(CronError::Fields);
        };
        let (minutes, _) = field(m, "minute", 0, 59)?;
        let (hours, _) = field(h, "hour", 0, 23)?;
        let (days, any_day) = field(dom, "day-of-month", 1, 31)?;
        let (months, _) = field(mon, "month", 1, 12)?;
        let (mut weekdays, any_weekday) = field(dow, "day-of-week", 0, 7)?;
        if weekdays & (1 << 7) != 0 {
            weekdays |= 1;
        }
        Ok(Self {
            minutes,
            hours: u32::try_from(hours).unwrap_or(0),
            days: u32::try_from(days).unwrap_or(0),
            months: u16::try_from(months).unwrap_or(0),
            weekdays: u8::try_from(weekdays & 0x7f).unwrap_or(0),
            any_day,
            any_weekday,
        })
    }

    fn day_matches(&self, year: i64, month: u32, day: u32, weekday: u32) -> bool {
        let _ = year;
        if self.months & (1 << month) == 0 {
            return false;
        }
        let dom = self.days & (1 << day) != 0;
        let dow = self.weekdays & (1 << weekday) != 0;
        match (self.any_day, self.any_weekday) {
            (true, true) => true,
            (true, false) => dow,
            (false, true) => dom,
            (false, false) => dom || dow,
        }
    }

    /// The first minute after `after` (Unix seconds) that matches; `None` when nothing
    /// matches in the next eight years (such as 31 February).
    #[must_use]
    pub fn next_after(&self, after: i64) -> Option<i64> {
        let start = after.div_euclid(60) * 60 + 60;
        let first_day = start.div_euclid(86_400);
        for offset in 0..(366 * 8) {
            let day = first_day + offset;
            let (year, month, dom) = civil_from_days(day);
            let weekday = u32::try_from((day + 4).rem_euclid(7)).unwrap_or(0);
            if !self.day_matches(year, month, dom, weekday) {
                continue;
            }
            let from = if offset == 0 { start - day * 86_400 } else { 0 };
            let from_minute = u32::try_from(from / 60).unwrap_or(0);
            for hour in (from_minute / 60)..24 {
                if self.hours & (1 << hour) == 0 {
                    continue;
                }
                let lowest = if hour == from_minute / 60 {
                    from_minute % 60
                } else {
                    0
                };
                if let Some(minute) = (lowest..60).find(|m| self.minutes & (1 << m) != 0) {
                    return Some(day * 86_400 + i64::from(hour) * 3600 + i64::from(minute) * 60);
                }
            }
        }
        None
    }
}

/// Year, month (1-12) and day of a day number counted from 1970-01-01.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let month = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// Which of the backups (`(id, created_at)`, Unix seconds, any order) retention lets go:
/// the newest backup of each of the `daily` most recent UTC days that have one is kept, the
/// newest of each of the `weekly` most recent weeks (Monday to Sunday) that have one, and
/// always the very newest. Everything else is returned, oldest first.
#[must_use]
pub fn expired<T: Copy + Eq>(backups: &[(T, i64)], daily: u32, weekly: u32) -> Vec<T> {
    let mut sorted: Vec<(T, i64)> = backups.to_vec();
    sorted.sort_by_key(|(_, at)| std::cmp::Reverse(*at));
    let mut keep: Vec<T> = Vec::new();
    if let Some((id, _)) = sorted.first() {
        keep.push(*id);
    }
    let mut days = 0u32;
    let mut weeks = 0u32;
    let (mut last_day, mut last_week) = (i64::MIN, i64::MIN);
    for (id, at) in &sorted {
        let day = at.div_euclid(86_400);
        // 1970-01-01 was a Thursday; weeks start on Monday.
        let week = (day + 3).div_euclid(7);
        if day != last_day {
            last_day = day;
            if days < daily {
                days += 1;
                keep.push(*id);
            }
        }
        if week != last_week {
            last_week = week;
            if weeks < weekly {
                weeks += 1;
                keep.push(*id);
            }
        }
    }
    sorted
        .iter()
        .rev()
        .map(|(id, _)| *id)
        .filter(|id| !keep.contains(id))
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // 2024-03-10 (a Sunday) 00:00:00 UTC.
    const SUNDAY: i64 = 1_710_028_800;

    #[test]
    fn a_daily_expression_finds_the_next_run() {
        let c = Cron::parse("30 2 * * *").unwrap();
        assert_eq!(c.next_after(SUNDAY), Some(SUNDAY + 2 * 3600 + 30 * 60));
        // Exactly at the time: the next one is tomorrow.
        assert_eq!(
            c.next_after(SUNDAY + 2 * 3600 + 30 * 60),
            Some(SUNDAY + 86_400 + 2 * 3600 + 30 * 60)
        );
    }

    #[test]
    fn steps_ranges_lists_and_weekdays() {
        let c = Cron::parse("*/15 9-10 * * 1,3").unwrap();
        // From Sunday: Monday 09:00.
        assert_eq!(c.next_after(SUNDAY), Some(SUNDAY + 86_400 + 9 * 3600));
        assert_eq!(
            c.next_after(SUNDAY + 86_400 + 9 * 3600),
            Some(SUNDAY + 86_400 + 9 * 3600 + 900)
        );
        // Sunday is 0 and 7.
        let s0 = Cron::parse("0 0 * * 0").unwrap();
        let s7 = Cron::parse("0 0 * * 7").unwrap();
        assert_eq!(s0, s7);
        assert_eq!(s0.next_after(SUNDAY), Some(SUNDAY + 7 * 86_400));
    }

    #[test]
    fn a_day_of_month_and_a_weekday_together_match_either() {
        // The 15th, or any Monday.
        let c = Cron::parse("0 0 15 * 1").unwrap();
        assert_eq!(c.next_after(SUNDAY), Some(SUNDAY + 86_400));
    }

    #[test]
    fn impossible_dates_and_bad_text_are_refused_or_never_match() {
        assert_eq!(Cron::parse("0 0 31 2 *").unwrap().next_after(SUNDAY), None);
        for bad in [
            "",
            "* * * *",
            "60 * * * *",
            "* 24 * * *",
            "* * 0 * *",
            "*/0 * * * *",
            "a * * * *",
            "5-1 * * * *",
        ] {
            assert!(Cron::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn leap_days_are_found() {
        let c = Cron::parse("0 12 29 2 *").unwrap();
        // 2024-02-29 12:00 UTC.
        assert_eq!(c.next_after(1_700_000_000), Some(1_709_208_000));
    }

    #[test]
    fn retention_keeps_the_newest_of_each_recent_day_and_week() {
        let day = 86_400;
        // Backups at 02:00 and 14:00 on twelve consecutive days from a Monday (2024-03-04).
        let monday = SUNDAY - 6 * day;
        let mut all = Vec::new();
        let mut id = 0u32;
        for d in 0..12 {
            for h in [2, 14] {
                all.push((id, monday + d * day + h * 3600));
                id += 1;
            }
        }
        let gone = expired(&all, 3, 2);
        let kept: Vec<u32> = all
            .iter()
            .map(|(i, _)| *i)
            .filter(|i| !gone.contains(i))
            .collect();
        // Newest of the last three days (days 11, 10, 9) and of the last two weeks (the newest
        // is day 11 = Friday of the second week; the week before is week one, newest Sunday).
        assert_eq!(kept, [13, 19, 21, 23], "{gone:?}");
        assert_eq!(gone.len(), 20);
        // Oldest first.
        assert!(gone.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn retention_never_empties_a_set_and_does_nothing_for_few_backups() {
        assert!(expired::<u32>(&[], 7, 4).is_empty());
        assert!(expired(&[(1u32, 100)], 0, 0).is_empty());
        assert_eq!(expired(&[(1u32, 100), (2, 200), (3, 300)], 0, 0), [1, 2]);
        assert!(expired(&[(1u32, 100), (2, 86_400 * 3)], 7, 4).is_empty());
    }
}
