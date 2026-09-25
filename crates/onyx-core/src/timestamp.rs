//! A minimal RFC 3339 reader, and calendar-date arithmetic.
//!
//! Deliberately hand-rolled rather than pulled from a date library. The engine needs
//! three things — the local calendar date a timestamp was written in, whether the
//! producer normalised it to UTC, and the distance in days between two dates — and none
//! of them require a time zone database. Avoiding one keeps the wasm build small and
//! keeps the crate free of the clock, which is what makes every test hermetic.

/// How a timestamp expressed its relationship to UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Offset {
    /// `Z`. The instant survives; the producer's local context does not.
    ///
    /// The schema's pattern permits this, but Principle 6 exists because an offset alone
    /// cannot answer which local day an instant belongs to — and `Z` does not even carry
    /// an offset.
    Utc,
    /// An explicit local offset, in minutes east of UTC.
    Local(i32),
}

/// A calendar date with no time and no zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CivilDate {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

impl CivilDate {
    /// Parses `YYYY-MM-DD`, rejecting dates that do not exist.
    pub fn parse(input: &str) -> Option<Self> {
        let bytes = input.as_bytes();
        if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
            return None;
        }
        // `parse::<u32>` accepts a leading `+`, and `-032-01-01` splits into a year of
        // `-032`, a month and a day that all parse — so a date no calendar has was accepted
        // as a real one. §3.2 wants four digits.
        if !bytes[..4].iter().all(u8::is_ascii_digit)
            || !bytes[5..7].iter().all(u8::is_ascii_digit)
            || !bytes[8..].iter().all(u8::is_ascii_digit)
        {
            return None;
        }
        let date = Self {
            year: input.get(0..4)?.parse().ok()?,
            month: input.get(5..7)?.parse().ok()?,
            day: input.get(8..10)?.parse().ok()?,
        };
        date.is_real().then_some(date)
    }

    fn is_real(&self) -> bool {
        (1..=12).contains(&self.month)
            && self.day >= 1
            && self.day <= days_in_month(self.year, self.month)
    }

    /// Days since 1970-01-01, so two dates can be compared and subtracted.
    pub fn day_number(&self) -> i64 {
        days_from_civil(self.year, self.month, self.day)
    }

    /// `self` minus `other`, in days.
    pub fn days_since(&self, other: &Self) -> i64 {
        self.day_number() - other.day_number()
    }
}

impl std::fmt::Display for CivilDate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

/// An RFC 3339 timestamp, read only as far as this engine needs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timestamp {
    pub date: CivilDate,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub offset: Offset,
}

impl Timestamp {
    /// Parses `YYYY-MM-DDTHH:MM:SS[.fff](Z|+HH:MM|-HH:MM)`.
    pub fn parse(input: &str) -> Option<Self> {
        let bytes = input.as_bytes();
        if bytes.len() < 20 {
            return None;
        }
        if !matches!(bytes[10], b'T' | b't') || bytes[13] != b':' || bytes[16] != b':' {
            return None;
        }

        let date = CivilDate::parse(input.get(0..10)?)?;
        let hour = two_digits(input, 11)?;
        let minute = two_digits(input, 14)?;
        // 60 is permitted: RFC 3339 allows a leap second.
        let second = two_digits(input, 17)?;
        if hour > 23 || minute > 59 || second > 60 {
            return None;
        }

        let mut rest = input.get(19..)?;
        if let Some(fraction) = rest.strip_prefix('.') {
            let digits = fraction.chars().take_while(char::is_ascii_digit).count();
            if digits == 0 {
                return None;
            }
            rest = rest.get(1 + digits..)?;
        }

        let offset = match rest {
            "Z" | "z" => Offset::Utc,
            _ => {
                let bytes = rest.as_bytes();
                if bytes.len() != 6 || bytes[3] != b':' {
                    return None;
                }
                let sign = match bytes[0] {
                    b'+' => 1,
                    b'-' => -1,
                    _ => return None,
                };
                let hours = two_digits(rest, 1)? as i32;
                let minutes = two_digits(rest, 4)? as i32;
                if hours > 23 || minutes > 59 {
                    return None;
                }
                Offset::Local(sign * (hours * 60 + minutes))
            }
        };

        Some(Self {
            date,
            hour,
            minute,
            second,
            offset,
        })
    }

    /// Whether the producer threw away its local context by normalising to UTC.
    /// Seconds since the same epoch `CivilDate::day_number` counts from, with the offset
    /// applied — so two timestamps written in different zones still compare correctly.
    pub fn instant(&self) -> i64 {
        let local = self.date.day_number() * 86_400
            + i64::from(self.hour) * 3_600
            + i64::from(self.minute) * 60
            + i64::from(self.second);
        let east = match self.offset {
            Offset::Local(minutes) => i64::from(minutes),
            Offset::Utc => 0,
        };
        local - east * 60
    }

    pub fn is_utc_normalised(&self) -> bool {
        matches!(self.offset, Offset::Utc)
    }
}

/// The two characters at `at`, read as a number only if both are ASCII digits.
///
/// `parse::<u32>` takes a sign, so `T+1:00:00` read as one o'clock and `+-3:00` as an
/// offset: the same generosity that let `-032-01-01` through as a date, one layer down.
/// Byte-indexed on purpose; a multi-byte character anywhere in the field fails here rather
/// than splitting a code point.
fn two_digits(input: &str, at: usize) -> Option<u32> {
    let pair = input.as_bytes().get(at..at + 2)?;
    if !pair.iter().all(u8::is_ascii_digit) {
        return None;
    }
    Some(u32::from(pair[0] - b'0') * 10 + u32::from(pair[1] - b'0'))
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Days since 1970-01-01. Howard Hinnant's civil-from-days, which is exact for every
/// proleptic Gregorian date and needs no lookup tables.
fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = i64::from(if month <= 2 { year - 1 } else { year });
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month = i64::from(month);
    let day_of_year =
        (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_an_offset_timestamp() {
        let stamp = Timestamp::parse("2026-08-10T08:30:00+03:00").unwrap();
        assert_eq!(
            stamp.date,
            CivilDate {
                year: 2026,
                month: 8,
                day: 10
            }
        );
        assert_eq!(stamp.hour, 8);
        assert_eq!(stamp.offset, Offset::Local(180));
        assert!(!stamp.is_utc_normalised());
    }

    #[test]
    fn notices_a_utc_normalised_timestamp() {
        let stamp = Timestamp::parse("2026-08-10T05:30:00Z").unwrap();
        assert!(stamp.is_utc_normalised());
    }

    #[test]
    fn reads_fractional_seconds_and_negative_offsets() {
        let stamp = Timestamp::parse("2026-08-10T08:30:00.123-04:30").unwrap();
        assert_eq!(stamp.offset, Offset::Local(-270));
    }

    #[test]
    fn rejects_a_timestamp_with_no_offset_at_all() {
        // The one shape the format cannot use: an instant with no relationship to UTC.
        assert!(Timestamp::parse("2026-08-10T08:30:00").is_none());
        assert!(Timestamp::parse("2026-08-10 08:30:00+03:00").is_none());
        assert!(Timestamp::parse("2026-08-10T08:30:00+0300").is_none());
    }

    #[test]
    fn rejects_dates_that_do_not_exist() {
        assert!(CivilDate::parse("2026-02-30").is_none());
        assert!(CivilDate::parse("2026-13-01").is_none());
        assert!(CivilDate::parse("2026-02-29").is_none());
        // 2028 is a leap year; 2100 is not, despite being divisible by four.
        assert!(CivilDate::parse("2028-02-29").is_some());
        assert!(CivilDate::parse("2100-02-29").is_none());
        assert!(CivilDate::parse("2000-02-29").is_some());
    }

    #[test]
    fn measures_the_distance_between_dates() {
        let earlier = CivilDate::parse("2026-08-10").unwrap();
        let later = CivilDate::parse("2026-08-11").unwrap();
        assert_eq!(later.days_since(&earlier), 1);
        assert_eq!(earlier.days_since(&later), -1);

        // Across a month, a year, and a leap day.
        let end_of_february = CivilDate::parse("2028-02-28").unwrap();
        let first_of_march = CivilDate::parse("2028-03-01").unwrap();
        assert_eq!(first_of_march.days_since(&end_of_february), 2);

        assert_eq!(CivilDate::parse("1970-01-01").unwrap().day_number(), 0);
    }
    /// Every position in a date is a digit. `parse::<u32>` is more generous — it takes a
    /// sign — so `-032-01-01` had a year, a month and a day that all parsed, and a date no
    /// calendar has passed validation in silence.
    #[test]
    fn a_date_is_digits_and_nothing_else() {
        assert!(CivilDate::parse("2026-08-10").is_some());

        for rejected in [
            "-032-01-01",
            "+032-01-01",
            "2026-+8-10",
            "2026-08-+9",
            "20a6-08-10",
            "2026-0八-10",
        ] {
            assert!(
                CivilDate::parse(rejected).is_none(),
                "{rejected:?} was accepted as a date"
            );
        }
    }

    /// The same rule for the clock and the offset. The Python reader parses these with a
    /// digits-only pattern, so without this the two implementations disagreed about whether
    /// such a document conforms, and no corpus case would have noticed.
    #[test]
    fn a_time_and_an_offset_are_digits_and_nothing_else() {
        assert!(Timestamp::parse("2026-08-10T08:30:00+03:00").is_some());

        for rejected in [
            "2026-08-10T+8:30:00+03:00",
            "2026-08-10T08:+3:00+03:00",
            "2026-08-10T08:30:+0+03:00",
            "2026-08-10T08:30:00++3:00",
            "2026-08-10T08:30:00+03:+0",
            "2026-08-10T08:30:00+-3:00",
            "2026-08-10T0八:30:00+03:00",
        ] {
            assert!(
                Timestamp::parse(rejected).is_none(),
                "{rejected:?} was accepted as a timestamp"
            );
        }
    }
}
