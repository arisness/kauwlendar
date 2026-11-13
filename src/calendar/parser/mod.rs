mod ics;
mod rrule;
mod timezone;

use super::event::CalendarEvent;
use chrono::{DateTime, Duration, FixedOffset, NaiveDate, NaiveDateTime, TimeZone, Timelike, Utc};
use ics::{RawEvent, clean_ical_text, parse_duration_seconds, parse_ics_components};
use rrule::{matches_rrule_date, parse_rrule};
use std::collections::{HashMap, HashSet};
use timezone::{IcsInstant, VTimezone, parse_ics_instant, resolve_tz_offset};

/// Parses an RFC 5545 ICS document and returns all active events occurring on `target_date`.
pub fn parse_ics_for_date(
    ics_raw: &str,
    cal_name: &str,
    target_date: NaiveDate,
    local_offset: &FixedOffset,
) -> Vec<CalendarEvent> {
    let (vtimezones, raw_events) = parse_ics_components(ics_raw);

    // Collect overrides from any VEVENT carrying RECURRENCE-ID (even if STATUS:CANCELLED)
    let mut overridden_instants: HashSet<(String, i64)> = HashSet::new();
    let mut overridden_dates: HashSet<(String, NaiveDate)> = HashSet::new();

    for ev in &raw_events {
        if ev.uid.is_empty() {
            continue;
        }
        if let Some((raw, tzid, is_date)) = &ev.recurrence_id {
            if let Some(inst) = parse_ics_instant(
                raw,
                tzid.as_deref(),
                *is_date,
                &vtimezones,
                local_offset,
            ) {
                match inst {
                    IcsInstant::AllDay(d) => {
                        overridden_dates.insert((ev.uid.clone(), d));
                    }
                    IcsInstant::Timed { utc, .. } => {
                        overridden_instants.insert((ev.uid.clone(), utc.timestamp()));
                    }
                }
            }
        }
    }

    let mut events = Vec::new();
    for ev in &raw_events {
        if ev.status == "CANCELLED" {
            continue;
        }
        if let Some(event) = evaluate_raw_event(
            ev,
            cal_name,
            target_date,
            local_offset,
            &vtimezones,
            &overridden_instants,
            &overridden_dates,
        ) {
            events.push(event);
        }
    }

    events
}

fn evaluate_raw_event(
    ev: &RawEvent,
    cal_name: &str,
    target_date: NaiveDate,
    local_offset: &FixedOffset,
    vtimezones: &HashMap<String, VTimezone>,
    overridden_instants: &HashSet<(String, i64)>,
    overridden_dates: &HashSet<(String, NaiveDate)>,
) -> Option<CalendarEvent> {
    let (dt_raw, dt_tzid, dt_is_date) = ev.dtstart.as_ref()?;
    let start_inst = parse_ics_instant(
        dt_raw,
        dt_tzid.as_deref(),
        *dt_is_date,
        vtimezones,
        local_offset,
    )?;

    let end_inst = ev.dtend.as_ref().and_then(|(raw, tzid, is_date)| {
        parse_ics_instant(raw, tzid.as_deref(), *is_date, vtimezones, local_offset)
    });
    let explicit_duration = if end_inst.is_none() && !ev.duration_raw.is_empty() {
        parse_duration_seconds(&ev.duration_raw)
    } else {
        None
    };

    // Parse EXDATEs into exact UTC instants (for timed) and date-only / local exclusions
    let mut ex_date_only: HashSet<NaiveDate> = HashSet::new();
    let mut ex_local_dates: HashSet<NaiveDate> = HashSet::new();
    let mut ex_rule_dates: HashSet<NaiveDate> = HashSet::new();
    let mut ex_utc_secs: HashSet<i64> = HashSet::new();

    for (raw, tzid, is_date) in &ev.exdates {
        if let Some(ex) = parse_ics_instant(
            raw,
            tzid.as_deref(),
            *is_date,
            vtimezones,
            local_offset,
        ) {
            match ex {
                IcsInstant::AllDay(d) => {
                    ex_date_only.insert(d);
                    ex_rule_dates.insert(d);
                    ex_local_dates.insert(d);
                }
                IcsInstant::Timed { utc, .. } => {
                    ex_rule_dates.insert(ex.date_in_rule_tz());
                    ex_local_dates.insert(ex.local_date(local_offset));
                    ex_utc_secs.insert(utc.timestamp());
                }
            }
        }
    }

    let is_standalone = (ev.rrule.is_empty() && ev.rdates.is_empty()) || ev.recurrence_id.is_some();

    if is_standalone {
        return evaluate_standalone(
            ev,
            cal_name,
            &start_inst,
            end_inst.as_ref(),
            explicit_duration,
            target_date,
            local_offset,
            &ex_date_only,
            &ex_utc_secs,
        );
    }

    // Recurring master VEVENT (RRULE and/or RDATE)
    match &start_inst {
        IcsInstant::AllDay(start_date) => {
            let span_days = match end_inst {
                Some(IcsInstant::AllDay(end_d)) if end_d > *start_date => {
                    (end_d - *start_date).num_days()
                }
                _ => match explicit_duration {
                    Some(secs) if secs >= 86400 => secs / 86400,
                    _ => 1,
                },
            };

            let rrule = if ev.rrule.is_empty() {
                None
            } else {
                parse_rrule(&ev.rrule, vtimezones, local_offset)
            };
            let rdates = parse_all_rdates(&ev.rdates, vtimezones, local_offset);

            let search_back = (span_days - 1).clamp(0, 366);
            for delta in 0..=search_back {
                let occ_start = target_date - Duration::days(delta);
                if ex_rule_dates.contains(&occ_start) || ex_local_dates.contains(&occ_start) {
                    continue;
                }
                if !ev.uid.is_empty() && overridden_dates.contains(&(ev.uid.clone(), occ_start)) {
                    continue;
                }
                let matches_rule = rrule
                    .as_ref()
                    .is_some_and(|r| matches_rrule_date(*start_date, None, occ_start, r));
                let matches_rdate = rdates
                    .iter()
                    .any(|rd| rd.local_date(local_offset) == occ_start);
                if matches_rule || matches_rdate {
                    return Some(build_all_day_event(cal_name, &ev.summary, &ev.description));
                }
            }
            None
        }
        IcsInstant::Timed {
            utc: dtstart_utc,
            wall_dt: start_wall,
        } => {
            let dur_secs = match end_inst.as_ref().and_then(|e| e.utc_datetime()) {
                Some(end_utc) if end_utc > *dtstart_utc => (end_utc - *dtstart_utc).num_seconds(),
                _ => explicit_duration.unwrap_or(0),
            };

            let rrule = if ev.rrule.is_empty() {
                None
            } else {
                parse_rrule(&ev.rrule, vtimezones, local_offset)
            };
            let rdates = parse_all_rdates(&ev.rdates, vtimezones, local_offset);

            let start_rule_date = start_wall.date();
            let start_wall_time = start_wall.time();
            let rule_tzid = if dt_raw.trim().ends_with('Z') {
                Some("UTC")
            } else {
                dt_tzid.as_deref()
            };

            // Any occurrence landing on `target_date` in `local_offset` must have its rule_date within [-2, +2] days
            if let Some(rule) = &rrule {
                for delta in -2..=2 {
                    let cand_rule_date = target_date + Duration::days(delta);
                    let cand_wall_dt = NaiveDateTime::new(cand_rule_date, start_wall_time);
                    let cand_tz_off =
                        resolve_tz_offset(rule_tzid, cand_wall_dt, vtimezones, local_offset);
                    let cand_utc = match cand_tz_off.from_local_datetime(&cand_wall_dt).earliest() {
                        Some(dt) => dt.with_timezone(&Utc),
                        None => match cand_tz_off
                            .from_local_datetime(&(cand_wall_dt + Duration::hours(1)))
                            .earliest()
                        {
                            Some(dt) => dt.with_timezone(&Utc),
                            None => DateTime::<Utc>::from_naive_utc_and_offset(cand_wall_dt, Utc),
                        },
                    };

                    let occ_local_dt = cand_utc.with_timezone(local_offset);
                    if occ_local_dt.date_naive() != target_date {
                        continue;
                    }

                    if !matches_rrule_date(start_rule_date, Some(cand_utc), cand_rule_date, rule) {
                        continue;
                    }

                    if ex_utc_secs.contains(&cand_utc.timestamp())
                        || ex_date_only.contains(&cand_rule_date)
                        || ex_date_only.contains(&target_date)
                    {
                        continue;
                    }

                    if !ev.uid.is_empty()
                        && (overridden_instants.contains(&(ev.uid.clone(), cand_utc.timestamp()))
                            || overridden_dates.contains(&(ev.uid.clone(), cand_rule_date)))
                    {
                        continue;
                    }

                    return Some(build_timed_event(
                        cal_name,
                        &ev.summary,
                        &ev.description,
                        occ_local_dt,
                        dur_secs,
                    ));
                }
            }

            // Check explicit RDATE instances
            for rdate in &rdates {
                if let Some(rd_utc) = rdate.utc_datetime() {
                    let occ_local_dt = rd_utc.with_timezone(local_offset);
                    if occ_local_dt.date_naive() != target_date {
                        continue;
                    }
                    if ex_utc_secs.contains(&rd_utc.timestamp())
                        || ex_date_only.contains(&target_date)
                    {
                        continue;
                    }
                    if !ev.uid.is_empty()
                        && overridden_instants.contains(&(ev.uid.clone(), rd_utc.timestamp()))
                    {
                        continue;
                    }
                    return Some(build_timed_event(
                        cal_name,
                        &ev.summary,
                        &ev.description,
                        occ_local_dt,
                        dur_secs,
                    ));
                }
            }

            None
        }
    }
}

fn parse_all_rdates(
    raw_rdates: &[(String, Option<String>, bool)],
    vtimezones: &HashMap<String, VTimezone>,
    local_offset: &FixedOffset,
) -> Vec<IcsInstant> {
    raw_rdates
        .iter()
        .filter_map(|(raw, tzid, is_date)| {
            parse_ics_instant(raw, tzid.as_deref(), *is_date, vtimezones, local_offset)
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn evaluate_standalone(
    ev: &RawEvent,
    cal_name: &str,
    start_inst: &IcsInstant,
    end_inst: Option<&IcsInstant>,
    explicit_duration: Option<i64>,
    target_date: NaiveDate,
    local_offset: &FixedOffset,
    ex_local_dates: &HashSet<NaiveDate>,
    ex_utc_secs: &HashSet<i64>,
) -> Option<CalendarEvent> {
    if ex_local_dates.contains(&target_date) {
        return None;
    }

    match start_inst {
        IcsInstant::AllDay(start_date) => {
            let end_exclusive = match end_inst {
                Some(IcsInstant::AllDay(end_d)) if end_d > start_date => *end_d,
                _ => match explicit_duration {
                    Some(secs) if secs >= 86400 => *start_date + Duration::days(secs / 86400),
                    _ => *start_date + Duration::days(1),
                },
            };

            if target_date >= *start_date && target_date < end_exclusive {
                Some(build_all_day_event(cal_name, &ev.summary, &ev.description))
            } else {
                None
            }
        }
        IcsInstant::Timed { utc: start_utc, .. } => {
            if ex_utc_secs.contains(&start_utc.timestamp()) {
                return None;
            }
            let local_start = start_utc.with_timezone(local_offset);
            if local_start.date_naive() != target_date {
                return None;
            }
            let dur_secs = match end_inst.and_then(|e| e.utc_datetime()) {
                Some(end_utc) if end_utc > *start_utc => (end_utc - *start_utc).num_seconds(),
                _ => explicit_duration.unwrap_or(0),
            };
            Some(build_timed_event(
                cal_name,
                &ev.summary,
                &ev.description,
                local_start,
                dur_secs,
            ))
        }
    }
}

fn build_all_day_event(cal_name: &str, summary: &str, description: &str) -> CalendarEvent {
    let mut ev = CalendarEvent::new_all_day(cal_name, clean_ical_text(summary));
    let clean_desc = clean_ical_text(description);
    if !clean_desc.trim().is_empty() {
        ev = ev.with_description(clean_desc);
    }
    ev
}

fn build_timed_event(
    cal_name: &str,
    summary: &str,
    description: &str,
    local_start: DateTime<FixedOffset>,
    dur_secs: i64,
) -> CalendarEvent {
    let start_mins = (local_start.hour() * 60 + local_start.minute()) as i32;
    let mut ev = CalendarEvent::new_timed(cal_name, clean_ical_text(summary), start_mins);

    if dur_secs > 0 {
        let local_end = local_start + Duration::seconds(dur_secs);
        let end_mins = if local_end.date_naive() > local_start.date_naive() {
            1439
        } else {
            (local_end.hour() * 60 + local_end.minute()) as i32
        };
        ev = ev.with_end_minutes(end_mins);
    }

    let clean_desc = clean_ical_text(description);
    if !clean_desc.trim().is_empty() {
        ev = ev.with_description(clean_desc);
    }
    ev
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_event_parsing() {
        let ics = r#"
BEGIN:VCALENDAR
BEGIN:VEVENT
DTSTART:20261108T140000Z
SUMMARY:Work Meeting
END:VEVENT
END:VCALENDAR
        "#;

        let target = NaiveDate::from_ymd_opt(2026, 11, 8).unwrap();
        let offset = FixedOffset::east_opt(-4 * 3600).unwrap(); // UTC-4: 14:00Z -> 10:00 AM local
        let events = parse_ics_for_date(ics, "Work", target, &offset);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].calendar_name, "Work");
        assert_eq!(events[0].summary, "Work Meeting");
        assert_eq!(events[0].start_minutes, 10 * 60); // 10:00
    }

    #[test]
    fn test_event_with_dtend_and_description() {
        let ics = r#"
BEGIN:VCALENDAR
BEGIN:VEVENT
DTSTART:20261108T100000Z
DTEND:20261108T114500Z
SUMMARY:Morning Routine
DESCRIPTION:Drink water and stretch
END:VEVENT
END:VCALENDAR
        "#;

        let target = NaiveDate::from_ymd_opt(2026, 11, 8).unwrap();
        let offset = FixedOffset::east_opt(-4 * 3600).unwrap(); // 10:00Z -> 06:00, 11:45Z -> 07:45
        let events = parse_ics_for_date(ics, "Personal", target, &offset);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].summary, "Morning Routine");
        assert_eq!(events[0].start_minutes, 6 * 60);
        assert_eq!(events[0].end_minutes, Some(7 * 60 + 45));
        assert_eq!(
            events[0].description,
            Some("Drink water and stretch".to_string())
        );
        assert_eq!(
            events[0].time_label(crate::config::TimeFormat::H12),
            "06:00 AM - 07:45 AM"
        );
    }

    #[test]
    fn test_rrule_weekly_recurrence() {
        let ics = r#"
BEGIN:VCALENDAR
BEGIN:VEVENT
DTSTART:20261001T150000Z
RRULE:FREQ=WEEKLY;BYDAY=SU,WE
SUMMARY:Gym Class
END:VEVENT
END:VCALENDAR
        "#;

        // 2026-11-08 is a Sunday (SU)
        let target_sunday = NaiveDate::from_ymd_opt(2026, 11, 8).unwrap();
        let offset = FixedOffset::east_opt(0).unwrap();
        let events = parse_ics_for_date(ics, "Personal", target_sunday, &offset);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].summary, "Gym Class");

        // 2026-11-09 is Monday (not in SU,WE)
        let target_monday = NaiveDate::from_ymd_opt(2026, 11, 9).unwrap();
        let events_monday = parse_ics_for_date(ics, "Personal", target_monday, &offset);
        assert_eq!(events_monday.len(), 0);
    }

    #[test]
    fn test_rrule_exdate_cancellation() {
        let ics = r#"
BEGIN:VCALENDAR
BEGIN:VEVENT
DTSTART:20261101T120000Z
RRULE:FREQ=DAILY
EXDATE:20261108T120000Z
SUMMARY:Daily Meeting
END:VEVENT
END:VCALENDAR
        "#;

        let target = NaiveDate::from_ymd_opt(2026, 11, 8).unwrap();
        let offset = FixedOffset::east_opt(0).unwrap();
        let events = parse_ics_for_date(ics, "Work", target, &offset);

        // Cancelled on 2026-11-08 by EXDATE
        assert_eq!(events.len(), 0);
    }

    #[test]
    fn test_all_day_event() {
        let ics = r#"
BEGIN:VCALENDAR
BEGIN:VEVENT
DTSTART;VALUE=DATE:20261108
SUMMARY:All-Day Event
END:VEVENT
END:VCALENDAR
        "#;

        let target = NaiveDate::from_ymd_opt(2026, 11, 8).unwrap();
        let offset = FixedOffset::east_opt(-4 * 3600).unwrap();
        let events = parse_ics_for_date(ics, "Personal", target, &offset);

        assert_eq!(events.len(), 1);
        assert!(events[0].is_all_day);
        assert_eq!(events[0].summary, "All-Day Event");
    }

    #[test]
    fn test_rrule_count_stops_finished_series() {
        let ics = r#"
BEGIN:VCALENDAR
BEGIN:VEVENT
DTSTART:20251006T140000Z
DTEND:20251006T150000Z
RRULE:FREQ=WEEKLY;COUNT=3;BYDAY=MO,WE
SUMMARY:Finished Course
END:VEVENT
END:VCALENDAR
        "#;
        // Occurrences: Mon 2025-10-06 (#1), Wed 2025-10-08 (#2), Mon 2025-10-13 (#3)
        let offset = FixedOffset::east_opt(0).unwrap();
        assert_eq!(
            parse_ics_for_date(ics, "Work", NaiveDate::from_ymd_opt(2025, 10, 13).unwrap(), &offset).len(),
            1
        );
        // Must NOT reappear later in Oct 2025 or in Nov 2026
        assert_eq!(
            parse_ics_for_date(ics, "Work", NaiveDate::from_ymd_opt(2025, 10, 15).unwrap(), &offset).len(),
            0
        );
        assert_eq!(
            parse_ics_for_date(ics, "Work", NaiveDate::from_ymd_opt(2026, 11, 11).unwrap(), &offset).len(),
            0
        );
    }

    #[test]
    fn test_status_cancelled_and_recurrence_id_overrides() {
        let ics = r#"
BEGIN:VCALENDAR
BEGIN:VEVENT
UID:series-1@google.com
DTSTART:20261106T140000Z
DTEND:20261106T150000Z
RRULE:FREQ=WEEKLY
SUMMARY:Weekly Sync
END:VEVENT
BEGIN:VEVENT
UID:series-1@google.com
RECURRENCE-ID:20261113T140000Z
DTSTART:20261114T160000Z
DTEND:20261114T170000Z
SUMMARY:Moved Weekly Sync
END:VEVENT
BEGIN:VEVENT
UID:series-1@google.com
RECURRENCE-ID:20261120T140000Z
DTSTART:20261120T140000Z
STATUS:CANCELLED
SUMMARY:Weekly Sync
END:VEVENT
BEGIN:VEVENT
UID:single-cancelled@google.com
DTSTART:20261113T180000Z
STATUS:CANCELLED
SUMMARY:Cancelled Event
END:VEVENT
END:VCALENDAR
        "#;

        let offset = FixedOffset::east_opt(0).unwrap();

        // 2026-11-06: normal master occurrence
        let evs_nov6 = parse_ics_for_date(ics, "Work", NaiveDate::from_ymd_opt(2026, 11, 6).unwrap(), &offset);
        assert_eq!(evs_nov6.len(), 1);
        assert_eq!(evs_nov6[0].summary, "Weekly Sync");

        // 2026-11-13: moved via RECURRENCE-ID + cancelled standalone -> 0 events
        let evs_nov13 = parse_ics_for_date(ics, "Work", NaiveDate::from_ymd_opt(2026, 11, 13).unwrap(), &offset);
        assert_eq!(evs_nov13.len(), 0);

        // 2026-11-14: receives the moved override event
        let evs_nov14 = parse_ics_for_date(ics, "Work", NaiveDate::from_ymd_opt(2026, 11, 14).unwrap(), &offset);
        assert_eq!(evs_nov14.len(), 1);
        assert_eq!(evs_nov14[0].summary, "Moved Weekly Sync");
        assert_eq!(evs_nov14[0].start_minutes, 16 * 60);

        // 2026-11-20: cancelled via RECURRENCE-ID override + STATUS:CANCELLED
        let evs_nov20 = parse_ics_for_date(ics, "Work", NaiveDate::from_ymd_opt(2026, 11, 20).unwrap(), &offset);
        assert_eq!(evs_nov20.len(), 0);
    }

    #[test]
    fn test_monthly_byday_ordinal_and_interval() {
        let ics = r#"
BEGIN:VCALENDAR
BEGIN:VEVENT
DTSTART;VALUE=DATE:20260112
RRULE:FREQ=MONTHLY;INTERVAL=2;BYDAY=2MO
SUMMARY:Bi-Monthly Second Monday
END:VEVENT
END:VCALENDAR
        "#;
        let offset = FixedOffset::east_opt(0).unwrap();

        // Jan 12, 2026 (2nd Mon, month 0) -> matches
        assert_eq!(
            parse_ics_for_date(ics, "Work", NaiveDate::from_ymd_opt(2026, 1, 12).unwrap(), &offset).len(),
            1
        );
        // Feb 9, 2026 (2nd Mon, odd month 1) -> skipped due to INTERVAL=2
        assert_eq!(
            parse_ics_for_date(ics, "Work", NaiveDate::from_ymd_opt(2026, 2, 9).unwrap(), &offset).len(),
            0
        );
        // Mar 12, 2026 (not 2nd Mon, just 12th of month!) -> must NOT match
        assert_eq!(
            parse_ics_for_date(ics, "Work", NaiveDate::from_ymd_opt(2026, 3, 12).unwrap(), &offset).len(),
            0
        );
        // Mar 9, 2026 (2nd Mon, month 2) -> matches
        assert_eq!(
            parse_ics_for_date(ics, "Work", NaiveDate::from_ymd_opt(2026, 3, 9).unwrap(), &offset).len(),
            1
        );
    }

    #[test]
    fn test_exdate_and_until_utc_across_local_midnight() {
        // Event at 22:00 local (UTC-4) = 02:00Z next day
        let ics = r#"
BEGIN:VCALENDAR
BEGIN:VEVENT
DTSTART:20261102T020000Z
DTEND:20261102T030000Z
RRULE:FREQ=DAILY;UNTIL=20261105T000000Z
EXDATE:20261104T020000Z
SUMMARY:Late Night Event
END:VEVENT
END:VCALENDAR
        "#;
        let offset = FixedOffset::east_opt(-4 * 3600).unwrap();

        // Starts 2026-11-01 22:00 local -> present
        assert_eq!(
            parse_ics_for_date(ics, "Work", NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(), &offset).len(),
            1
        );
        // 2026-11-03 22:00 local = 2026-11-04 02:00Z -> excluded by EXDATE
        assert_eq!(
            parse_ics_for_date(ics, "Work", NaiveDate::from_ymd_opt(2026, 11, 3).unwrap(), &offset).len(),
            0
        );
        // 2026-11-04 22:00 local = 2026-11-05 02:00Z > UNTIL (2026-11-05 00:00Z) -> must NOT match
        assert_eq!(
            parse_ics_for_date(ics, "Work", NaiveDate::from_ymd_opt(2026, 11, 4).unwrap(), &offset).len(),
            0
        );
    }

    #[test]
    fn test_vtimezone_and_valarm_isolation() {
        let ics = r#"
BEGIN:VCALENDAR
BEGIN:VTIMEZONE
TZID:America/New_York
BEGIN:DAYLIGHT
TZOFFSETFROM:-0500
TZOFFSETTO:-0400
DTSTART:19700308T020000
RRULE:FREQ=YEARLY;BYMONTH=3;BYDAY=2SU
END:DAYLIGHT
BEGIN:STANDARD
TZOFFSETFROM:-0400
TZOFFSETTO:-0500
DTSTART:19701101T020000
RRULE:FREQ=YEARLY;BYMONTH=11;BYDAY=1SU
END:STANDARD
END:VTIMEZONE
BEGIN:VEVENT
DTSTART;TZID=America/New_York:20261113T090000
DTEND;TZID=America/New_York:20261113T100000
SUMMARY:NY Morning Call
DESCRIPTION:Real event description
BEGIN:VALARM
ACTION:DISPLAY
SUMMARY:Alarm Summary Must Be Ignored
DESCRIPTION:This is an event reminder
TRIGGER:-PT10M
END:VALARM
END:VEVENT
END:VCALENDAR
        "#;

        // 2026-11-13 in America/New_York is EST (-0500). Viewed from UTC-4 -> 10:00 AM local.
        let offset = FixedOffset::east_opt(-4 * 3600).unwrap();
        let events = parse_ics_for_date(ics, "Work", NaiveDate::from_ymd_opt(2026, 11, 13).unwrap(), &offset);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].summary, "NY Morning Call");
        assert_eq!(events[0].description.as_deref(), Some("Real event description"));
        assert_eq!(events[0].start_minutes, 10 * 60);
        assert_eq!(events[0].end_minutes, Some(11 * 60));
    }

    #[test]
    fn test_weekly_interval_with_wkst_sunday() {
        let ics = r#"
BEGIN:VCALENDAR
BEGIN:VEVENT
DTSTART;VALUE=DATE:20261107
RRULE:FREQ=WEEKLY;INTERVAL=2;WKST=SU;BYDAY=SU,SA
SUMMARY:Biweekly Weekend
END:VEVENT
END:VCALENDAR
        "#;
        let offset = FixedOffset::east_opt(0).unwrap();

        // Week of Sun Nov 1..Sat Nov 7: Sat Nov 7 matches
        assert_eq!(
            parse_ics_for_date(ics, "Personal", NaiveDate::from_ymd_opt(2026, 11, 7).unwrap(), &offset).len(),
            1
        );
        // Week of Sun Nov 8..Sat Nov 14 (odd week): Sun Nov 8 must NOT match
        assert_eq!(
            parse_ics_for_date(ics, "Personal", NaiveDate::from_ymd_opt(2026, 11, 8).unwrap(), &offset).len(),
            0
        );
        // Week of Sun Nov 15..Sat Nov 21 (even week): Sun Nov 15 & Sat Nov 21 both match
        assert_eq!(
            parse_ics_for_date(ics, "Personal", NaiveDate::from_ymd_opt(2026, 11, 15).unwrap(), &offset).len(),
            1
        );
        assert_eq!(
            parse_ics_for_date(ics, "Personal", NaiveDate::from_ymd_opt(2026, 11, 21).unwrap(), &offset).len(),
            1
        );
    }

    #[test]
    fn test_multiday_all_day_exclusive_dtend() {
        let ics = r#"
BEGIN:VCALENDAR
BEGIN:VEVENT
DTSTART;VALUE=DATE:20261110
DTEND;VALUE=DATE:20261113
SUMMARY:Conference
END:VEVENT
END:VCALENDAR
        "#;
        let offset = FixedOffset::east_opt(0).unwrap();
        assert_eq!(
            parse_ics_for_date(ics, "Work", NaiveDate::from_ymd_opt(2026, 11, 10).unwrap(), &offset).len(),
            1
        );
        assert_eq!(
            parse_ics_for_date(ics, "Work", NaiveDate::from_ymd_opt(2026, 11, 12).unwrap(), &offset).len(),
            1
        );
        assert_eq!(
            parse_ics_for_date(ics, "Work", NaiveDate::from_ymd_opt(2026, 11, 13).unwrap(), &offset).len(),
            0
        );
    }
}
