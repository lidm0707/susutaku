//! Minimal 5-field cron engine: minute hour day-of-month month day-of-week.
//! Pure std; no external time crate. Times are UTC unix epoch seconds.

pub const MINUTE_MIN: u32 = 0;
pub const MINUTE_MAX: u32 = 59;
pub const HOUR_MIN: u32 = 0;
pub const HOUR_MAX: u32 = 23;
pub const DOM_MIN: u32 = 1;
pub const DOM_MAX: u32 = 31;
pub const MONTH_MIN: u32 = 1;
pub const MONTH_MAX: u32 = 12;
pub const DOW_MIN: u32 = 0;
pub const DOW_MAX: u32 = 7;

pub const FIELD_COUNT: usize = 5;

/// Scan window for `next_after`: one leap year of minutes.
pub const MAX_SCAN_MINUTES: u64 = 366 * 24 * 60;
pub const MINUTE: u64 = 60;
pub const HOUR: u64 = 60 * MINUTE;
pub const DAY: u64 = 24 * HOUR;

/// Unix weekday of 1970-01-01 (Thursday).
const EPOCH_WEEKDAY: u32 = 4;

#[derive(Debug, PartialEq, Eq)]
pub struct CronError {
    pub field: &'static str,
    pub value: String,
}

impl std::fmt::Display for CronError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "bad cron field {} {:?}", self.field, self.value)
    }
}

impl std::error::Error for CronError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cron {
    minute: Field<60>,
    hour: Field<24>,
    dom: Field<32>,
    month: Field<13>,
    dow: Field<7>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Field<const N: usize> {
    bits: [bool; N],
}

impl<const N: usize> Field<N> {
    fn none() -> Self {
        Self { bits: [false; N] }
    }

    fn set(&mut self, v: u32) -> bool {
        if v < N as u32 {
            self.bits[v as usize] = true;
            true
        } else {
            false
        }
    }

    fn contains(&self, v: u32) -> bool {
        self.bits[v as usize % N]
    }

    fn is_empty(&self) -> bool {
        self.bits.iter().all(|b| !b)
    }
}

impl Cron {
    pub fn parse(expr: &str) -> Result<Self, CronError> {
        let parts: Vec<&str> = expr.split_whitespace().collect();
        if parts.len() != FIELD_COUNT {
            return Err(CronError {
                field: "expression",
                value: expr.to_owned(),
            });
        }
        Ok(Self {
            minute: parse_sized(parts[0], "minute", MINUTE_MIN, MINUTE_MAX)?,
            hour: parse_sized(parts[1], "hour", HOUR_MIN, HOUR_MAX)?,
            dom: parse_sized(parts[2], "day-of-month", DOM_MIN, DOM_MAX)?,
            month: parse_sized(parts[3], "month", MONTH_MIN, MONTH_MAX)?,
            dow: parse_sized(parts[4], "day-of-week", DOW_MIN, DOW_MAX)?,
        })
    }

    /// First minute strictly after `epoch` that matches; capped scan.
    pub fn next_after(&self, epoch: u64) -> u64 {
        let mut t = epoch - epoch % MINUTE + MINUTE;
        for _ in 0..MAX_SCAN_MINUTES {
            if self.matches(t) {
                return t;
            }
            let (month, dom) = month_day(t);
            let month_len = days_in_month(year_of(t), month);
            if !self.month.contains(month) {
                // jump to first minute of next month
                t += u64::from(month_len - dom + 1) * DAY - t % DAY;
            } else if !self.dom.contains(dom) || !self.dow.contains(weekday(t)) {
                t += DAY - t % DAY;
            } else {
                t += MINUTE;
            }
        }
        u64::MAX
    }

    /// Whether the given unix time (any second within the minute) matches.
    pub fn matches(&self, t: u64) -> bool {
        let (month, dom) = month_day(t);
        self.minute.contains(minute_of(t))
            && self.hour.contains(hour_of(t))
            && self.month.contains(month)
            && self.dom.contains(dom)
            && self.dow.contains(weekday(t))
    }

    pub fn is_empty(&self) -> bool {
        self.minute.is_empty()
    }
}

fn parse_sized<const N: usize>(
    part: &str,
    field_name: &'static str,
    min: u32,
    max: u32,
) -> Result<Field<N>, CronError> {
    let mut out = Field::none();
    for term in part.split(',') {
        let (range, step) = match term.split_once('/') {
            Some((r, s)) => {
                let step: u32 = s.parse().map_err(|_| bad(field_name, part))?;
                if step == 0 {
                    return Err(bad(field_name, part));
                }
                (r, step)
            }
            None => (term, 1),
        };
        let (lo, hi) = match range.split_once('-') {
            Some((a, b)) => {
                let lo: u32 = a.parse().map_err(|_| bad(field_name, part))?;
                let hi: u32 = b.parse().map_err(|_| bad(field_name, part))?;
                (lo, hi)
            }
            None => {
                if range == "*" {
                    (min, max)
                } else {
                    let v: u32 = range.parse().map_err(|_| bad(field_name, part))?;
                    if step > 1 { (v, max) } else { (v, v) }
                }
            }
        };
        if lo < min || hi > max || lo > hi {
            return Err(bad(field_name, part));
        }
        let mut v = lo;
        loop {
            // cron dow: 7 is an alias for Sunday (0)
            let value = if max == DOW_MAX && v == DOW_MAX { 0 } else { v };
            if !out.set(value) {
                return Err(bad(field_name, part));
            }
            if v + step > hi {
                break;
            }
            v += step;
        }
    }
    Ok(out)
}

fn bad(field: &'static str, value: &str) -> CronError {
    CronError {
        field,
        value: value.to_owned(),
    }
}

pub fn minute_of(t: u64) -> u32 {
    ((t / MINUTE) % 60) as u32
}

pub fn hour_of(t: u64) -> u32 {
    ((t / HOUR) % 24) as u32
}

pub fn weekday(t: u64) -> u32 {
    ((t / DAY + u64::from(EPOCH_WEEKDAY)) % 7) as u32
}

pub fn year_of(t: u64) -> i64 {
    civil_from_days((t / DAY) as i64).0
}

pub fn month_day(t: u64) -> (u32, u32) {
    let (_, m, d) = civil_from_days((t / DAY) as i64);
    (m, d)
}

/// Howard Hinnant's civil_from_days: (year, month, day) from days since epoch.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    const DAYS_TO_0000_03_01: i64 = 719_468;
    let z = days + DAYS_TO_0000_03_01;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m as u32, d as u32)
}

pub fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap(year) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}
