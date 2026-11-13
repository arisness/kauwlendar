use super::rrule::{
    Freq, candidate_months_for_yearly, matching_dates_in_month, parse_rrule, within_until,
};
use chrono::{
    DateTime, Datelike, Duration, FixedOffset, NaiveDate, NaiveDateTime, TimeZone, Utc,
};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub(super) struct TzObservance {
    pub(super) dtstart: NaiveDateTime,
    pub(super) offset_from: FixedOffset,
    pub(super) offset_to: FixedOffset,
    pub(super) rrule: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct VTimezone {
    pub(super) observances: Vec<TzObservance>,
}

impl VTimezone {
    fn offset_at(&self, wall_dt: NaiveDateTime) -> Option<FixedOffset> {
        if self.observances.is_empty() {
            return None;
        }

        let mut best_transition: Option<(NaiveDateTime, FixedOffset)> = None;
        let mut earliest_from: Option<(NaiveDateTime, FixedOffset)> = None;

        for obs in &self.observances {
            match &earliest_from {
                None => earliest_from = Some((obs.dtstart, obs.offset_from)),
                Some((t, _)) if obs.dtstart < *t => {
                    earliest_from = Some((obs.dtstart, obs.offset_from));
                }
                _ => {}
            }

            if let Some(trans_dt) = latest_observance_transition(obs, wall_dt) {
                match best_transition {
                    None => best_transition = Some((trans_dt, obs.offset_to)),
                    Some((prev_dt, _)) if trans_dt > prev_dt => {
                        best_transition = Some((trans_dt, obs.offset_to));
                    }
                    _ => {}
                }
            }
        }

        best_transition
            .map(|(_, off)| off)
            .or_else(|| earliest_from.map(|(_, off)| off))
    }
}

fn latest_observance_transition(obs: &TzObservance, wall_dt: NaiveDateTime) -> Option<NaiveDateTime> {
    if wall_dt < obs.dtstart {
        return None;
    }

    let Some(rrule_str) = obs.rrule.as_deref() else {
        return Some(obs.dtstart);
    };

    let empty_tzs = HashMap::new();
    let rule = parse_rrule(rrule_str, &empty_tzs, &obs.offset_from)?;

    // VTIMEZONE rules are almost always YEARLY (with BYMONTH + BYDAY/BYMONTHDAY).
    if rule.freq == Freq::Yearly {
        let mut latest = Some(obs.dtstart);
        let time = obs.dtstart.time();
        let target_year = wall_dt.year();
        for year in (target_year - 1..=target_year).rev() {
            if year < obs.dtstart.year() {
                continue;
            }
            if let Some(months) = candidate_months_for_yearly(obs.dtstart.date(), &rule) {
                for m in months {
                    for d in matching_dates_in_month(obs.dtstart.date(), year, m, &rule) {
                        if d < obs.dtstart.date() {
                            continue;
                        }
                        let cand = NaiveDateTime::new(d, time);
                        if cand <= wall_dt {
                            if let Some(until) = &rule.until {
                                let cand_utc = match obs.offset_from.from_local_datetime(&cand).earliest() {
                                    Some(dt) => dt.with_timezone(&Utc),
                                    None => DateTime::<Utc>::from_naive_utc_and_offset(cand, Utc),
                                };
                                if !within_until(d, Some(cand_utc), until) {
                                    continue;
                                }
                            }
                            if latest.is_none_or(|prev| cand > prev) {
                                latest = Some(cand);
                            }
                        }
                    }
                }
            }
        }
        return latest;
    }

    Some(obs.dtstart)
}

#[derive(Debug, Clone)]
pub(super) enum IcsInstant {
    AllDay(NaiveDate),
    Timed {
        utc: DateTime<Utc>,
        wall_dt: NaiveDateTime,
    },
}

impl IcsInstant {
    pub(super) fn date_in_rule_tz(&self) -> NaiveDate {
        match self {
            IcsInstant::AllDay(d) => *d,
            IcsInstant::Timed { wall_dt, .. } => wall_dt.date(),
        }
    }

    pub(super) fn local_date(&self, local_offset: &FixedOffset) -> NaiveDate {
        match self {
            IcsInstant::AllDay(d) => *d,
            IcsInstant::Timed { utc, .. } => utc.with_timezone(local_offset).date_naive(),
        }
    }

    pub(super) fn utc_datetime(&self) -> Option<DateTime<Utc>> {
        match self {
            IcsInstant::AllDay(_) => None,
            IcsInstant::Timed { utc, .. } => Some(*utc),
        }
    }
}

pub(super) fn parse_tz_offset(raw: &str) -> Option<FixedOffset> {
    let s = raw.trim();
    if s.len() < 5 {
        return None;
    }
    let sign = match s.as_bytes()[0] {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let digits = &s[1..];
    if !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let hours: i32 = digits.get(0..2)?.parse().ok()?;
    let mins: i32 = digits.get(2..4)?.parse().ok()?;
    let secs: i32 = if digits.len() >= 6 {
        digits.get(4..6)?.parse().ok()?
    } else {
        0
    };
    FixedOffset::east_opt(sign * (hours * 3600 + mins * 60 + secs))
}

pub(super) fn resolve_tz_offset(
    tzid: Option<&str>,
    wall_dt: NaiveDateTime,
    vtimezones: &HashMap<String, VTimezone>,
    local_offset: &FixedOffset,
) -> FixedOffset {
    let Some(id) = tzid else {
        return *local_offset;
    };
    if id.eq_ignore_ascii_case("UTC")
        || id.eq_ignore_ascii_case("Etc/UTC")
        || id.eq_ignore_ascii_case("GMT")
    {
        return FixedOffset::east_opt(0).unwrap();
    }
    if let Some(vtz) = vtimezones.get(id) {
        if let Some(off) = vtz.offset_at(wall_dt) {
            return off;
        }
    }
    *local_offset
}

pub(super) fn parse_ics_instant(
    raw: &str,
    tzid: Option<&str>,
    is_value_date: bool,
    vtimezones: &HashMap<String, VTimezone>,
    local_offset: &FixedOffset,
) -> Option<IcsInstant> {
    let clean = raw.trim();

    if is_value_date || (clean.len() == 8 && clean.chars().all(|c| c.is_ascii_digit())) {
        let date_str = clean.get(..8)?;
        let date = NaiveDate::parse_from_str(date_str, "%Y%m%d").ok()?;
        return Some(IcsInstant::AllDay(date));
    }

    if clean.ends_with('Z') && clean.len() >= 16 {
        let trimmed = clean.trim_end_matches('Z');
        let naive = NaiveDateTime::parse_from_str(trimmed, "%Y%m%dT%H%M%S").ok()?;
        let utc = DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc);
        return Some(IcsInstant::Timed {
            utc,
            wall_dt: naive,
        });
    }

    if clean.len() >= 15 && clean.contains('T') {
        let slice = &clean[..15];
        let wall_dt = NaiveDateTime::parse_from_str(slice, "%Y%m%dT%H%M%S").ok()?;
        let tz_off = resolve_tz_offset(tzid, wall_dt, vtimezones, local_offset);
        let utc = match tz_off.from_local_datetime(&wall_dt).earliest() {
            Some(dt) => dt.with_timezone(&Utc),
            None => match tz_off.from_local_datetime(&(wall_dt + Duration::hours(1))).earliest() {
                Some(dt) => dt.with_timezone(&Utc),
                None => DateTime::<Utc>::from_naive_utc_and_offset(wall_dt, Utc),
            },
        };
        return Some(IcsInstant::Timed {
            utc,
            wall_dt,
        });
    }

    None
}
