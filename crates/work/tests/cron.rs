use work::services::cron::Cron;

/// 2026-01-05 00:00:00 UTC — a Monday. Offsets may exceed 24h (next day).
fn secs(offset: &str) -> u64 {
    1767571200
        + offset
            .split(':')
            .map(|p| p.parse::<u64>().expect("time part"))
            .zip([3600, 60, 1])
            .map(|(v, mul)| v * mul)
            .sum::<u64>()
}

#[test]
fn parses_wildcard_and_matches_every_minute() {
    let c = Cron::parse("* * * * *").expect("parse");
    assert_eq!(c.next_after(secs("04:00:30")), secs("04:01:00"));
}

#[test]
fn step_minutes() {
    let c = Cron::parse("*/15 * * * *").expect("parse");
    assert_eq!(c.next_after(secs("04:00:00")), secs("04:15:00"));
    assert_eq!(c.next_after(secs("04:15:01")), secs("04:30:00"));
}

#[test]
fn list_and_range() {
    let c = Cron::parse("5,10 1-2 * * *").expect("parse");
    assert_eq!(c.next_after(secs("00:00:00")), secs("01:05:00"));
    assert_eq!(c.next_after(secs("01:05:00")), secs("01:10:00"));
    assert_eq!(c.next_after(secs("01:10:00")), secs("02:05:00"));
    assert_eq!(c.next_after(secs("02:10:00")), secs("25:05:00"));
}

#[test]
fn specific_hour_single_value() {
    let c = Cron::parse("0 3 * * *").expect("parse");
    assert_eq!(c.next_after(secs("04:00:00")), secs("27:00:00"));
}

#[test]
fn day_of_month() {
    let c = Cron::parse("0 0 10 * *").expect("parse");
    assert_eq!(c.next_after(secs("04:00:00")), secs("120:00:00"));
}

#[test]
fn day_of_week_seven_is_sunday() {
    let sunday = Cron::parse("0 12 * * 7").expect("parse");
    let zero = Cron::parse("0 12 * * 0").expect("parse");
    // both are the upcoming Sunday 12:00 after Monday 04:00
    assert_eq!(
        sunday.next_after(secs("04:00:00")),
        zero.next_after(secs("04:00:00"))
    );
}

#[test]
fn month_jump_skips_days() {
    let c = Cron::parse("0 0 1 2 *").expect("parse");
    // next Feb 1st after 2026-01-05
    assert_eq!(c.next_after(secs("04:00:00")), secs("648:00:00"));
}

#[test]
fn rejects_bad_expressions() {
    for expr in [
        "",
        "* * * *",
        "61 * * * *",
        "* 24 * * *",
        "* * 0 * *",
        "* * * 13 *",
        "a * * * *",
        "*/0 * * * *",
        "5-2 * * * *",
    ] {
        assert!(Cron::parse(expr).is_err(), "should reject {expr:?}");
    }
}

#[test]
fn error_reports_field() {
    let err = Cron::parse("61 * * * *").expect_err("bad minute");
    assert_eq!(err.field, "minute");
    assert_eq!(err.value, "61");
}
