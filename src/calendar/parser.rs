use super::event::CalendarEvent;
use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, NaiveDateTime, Timelike, Utc, Weekday};

pub fn parse_ics_for_date(
    ics_raw: &str,
    cal_name: &str,
    target_date: NaiveDate,
    local_offset: &FixedOffset,
) -> Vec<CalendarEvent> {
    let unfolded = unfold_ics(ics_raw);
    let mut events = Vec::new();

    let mut in_event = false;
    let mut dtstart_raw = String::new();
    let mut dtend_raw = String::new();
    let mut summary = String::new();
    let mut description_raw = String::new();
    let mut rrule_raw = String::new();
    let mut exdates = Vec::new();

    for line in unfolded.lines() {
        let line = line.trim_end_matches('\r').trim();
        if line.is_empty() {
            continue;
        }

        if line == "BEGIN:VEVENT" {
            in_event = true;
            dtstart_raw.clear();
            dtend_raw.clear();
            summary.clear();
            description_raw.clear();
            rrule_raw.clear();
            exdates.clear();
        } else if line == "END:VEVENT" && in_event {
            in_event = false;
            if let Some(event) = evaluate_event(
                cal_name,
                &dtstart_raw,
                &dtend_raw,
                &summary,
                &description_raw,
                &rrule_raw,
                &exdates,
                target_date,
                local_offset,
            ) {
                events.push(event);
            }
        } else if in_event {
            if line.starts_with("DTSTART") {
                if let Some((_, val)) = line.split_once(':') {
                    dtstart_raw = val.trim().to_string();
                }
            } else if line.starts_with("DTEND") {
                if let Some((_, val)) = line.split_once(':') {
                    dtend_raw = val.trim().to_string();
                }
            } else if line.starts_with("SUMMARY") {
                if let Some((_, val)) = line.split_once(':') {
                    summary = val.trim().to_string();
                }
            } else if line.starts_with("DESCRIPTION") {
                if let Some((_, val)) = line.split_once(':') {
                    description_raw = val.trim().to_string();
                }
            } else if line.starts_with("RRULE:") {
                rrule_raw = line.trim_start_matches("RRULE:").trim().to_string();
            } else if line.starts_with("EXDATE") {
                if let Some((_, val)) = line.split_once(':') {
                    for date_part in val.split(',') {
                        let clean_date = date_part.trim();
                        if clean_date.len() >= 8 {
                            if let Ok(d) = NaiveDate::parse_from_str(&clean_date[..8], "%Y%m%d") {
                                exdates.push(d);
                            }
                        }
                    }
                }
            }
        }
    }

    events
}

fn unfold_ics(raw: &str) -> String {
    raw.replace("\r\n ", "")
        .replace("\n ", "")
        .replace("\r\n\t", "")
        .replace("\n\t", "")
}

fn evaluate_event(
    cal_name: &str,
    dtstart_raw: &str,
    dtend_raw: &str,
    summary_raw: &str,
    description_raw: &str,
    rrule_raw: &str,
    exdates: &[NaiveDate],
    target_date: NaiveDate,
    local_offset: &FixedOffset,
) -> Option<CalendarEvent> {
    if dtstart_raw.is_empty() {
        return None;
    }

    // Si la fecha actual está en la lista de exclusiones (eventos cancelados)
    if exdates.contains(&target_date) {
        return None;
    }

    let parsed_dt = parse_dtstart(dtstart_raw, local_offset)?;
    let clean_summary = clean_ical_text(summary_raw);
    let clean_desc = clean_ical_text(description_raw);

    // Calcular hora final en minutos si DTEND está presente
    let end_minutes = if !dtend_raw.is_empty() {
        parse_dtstart(dtend_raw, local_offset).and_then(|p| match p {
            ParsedDateTime::Timed(_, m) => Some(m),
            _ => None,
        })
    } else {
        None
    };

    match parsed_dt {
        ParsedDateTime::AllDay(start_date) => {
            let occurs_today = if rrule_raw.is_empty() {
                start_date == target_date
            } else {
                matches_rrule(start_date, target_date, rrule_raw)
            };

            if occurs_today {
                let mut ev = CalendarEvent::new_all_day(cal_name, clean_summary);
                if !clean_desc.trim().is_empty() {
                    ev = ev.with_description(clean_desc);
                }
                Some(ev)
            } else {
                None
            }
        }
        ParsedDateTime::Timed(start_date, minutes) => {
            let occurs_today = if rrule_raw.is_empty() {
                start_date == target_date
            } else {
                matches_rrule(start_date, target_date, rrule_raw)
            };

            if occurs_today {
                let mut ev = CalendarEvent::new_timed(cal_name, clean_summary, minutes);
                if let Some(em) = end_minutes {
                    ev = ev.with_end_minutes(em);
                }
                if !clean_desc.trim().is_empty() {
                    ev = ev.with_description(clean_desc);
                }
                Some(ev)
            } else {
                None
            }
        }
    }
}

enum ParsedDateTime {
    AllDay(NaiveDate),
    Timed(NaiveDate, i32),
}

fn parse_dtstart(raw: &str, local_offset: &FixedOffset) -> Option<ParsedDateTime> {
    let clean = raw.trim();

    // Caso 1: Todo el día (YYYYMMDD)
    if clean.len() == 8 && clean.chars().all(|c| c.is_ascii_digit()) {
        let date = NaiveDate::parse_from_str(clean, "%Y%m%d").ok()?;
        return Some(ParsedDateTime::AllDay(date));
    }

    // Caso 2: UTC con sufijo Z (ej. 20261108T143000Z)
    if clean.ends_with('Z') && clean.len() >= 16 {
        let trimmed = clean.trim_end_matches('Z');
        let naive = NaiveDateTime::parse_from_str(trimmed, "%Y%m%dT%H%M%S").ok()?;
        let utc_dt = DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc);
        let local_dt = utc_dt.with_timezone(local_offset);
        let minutes = (local_dt.hour() * 60 + local_dt.minute()) as i32;
        return Some(ParsedDateTime::Timed(local_dt.date_naive(), minutes));
    }

    // Caso 3: Fecha y hora local (ej. 20261108T143000)
    if clean.len() >= 15 && clean.contains('T') {
        let slice = &clean[..15];
        let naive = NaiveDateTime::parse_from_str(slice, "%Y%m%dT%H%M%S").ok()?;
        let minutes = (naive.hour() * 60 + naive.minute()) as i32;
        return Some(ParsedDateTime::Timed(naive.date(), minutes));
    }

    None
}

fn matches_rrule(start_date: NaiveDate, target_date: NaiveDate, rrule: &str) -> bool {
    // Si la fecha objetivo es anterior a la fecha de inicio del evento, aún no comienza
    if target_date < start_date {
        return false;
    }

    let mut freq = "";
    let mut interval: i64 = 1;
    let mut until_date: Option<NaiveDate> = None;
    let mut byday: Vec<Weekday> = Vec::new();

    for part in rrule.split(';') {
        if let Some((k, v)) = part.split_once('=') {
            match k.trim() {
                "FREQ" => freq = v.trim(),
                "INTERVAL" => {
                    if let Ok(n) = v.trim().parse::<i64>() {
                        interval = n.max(1);
                    }
                }
                "UNTIL" => {
                    let v_clean = v.trim();
                    if v_clean.len() >= 8 {
                        if let Ok(d) = NaiveDate::parse_from_str(&v_clean[..8], "%Y%m%d") {
                            until_date = Some(d);
                        }
                    }
                }
                "BYDAY" => {
                    for day_str in v.split(',') {
                        let code = day_str
                            .trim()
                            .trim_start_matches(|c: char| c.is_ascii_digit() || c == '-');
                        match code {
                            "MO" => byday.push(Weekday::Mon),
                            "TU" => byday.push(Weekday::Tue),
                            "WE" => byday.push(Weekday::Wed),
                            "TH" => byday.push(Weekday::Thu),
                            "FR" => byday.push(Weekday::Fri),
                            "SA" => byday.push(Weekday::Sat),
                            "SU" => byday.push(Weekday::Sun),
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
    }

    if let Some(until) = until_date {
        if target_date > until {
            return false;
        }
    }

    match freq {
        "DAILY" => {
            let diff_days = (target_date - start_date).num_days();
            diff_days >= 0 && diff_days % interval == 0
        }
        "WEEKLY" => {
            let diff_days = (target_date - start_date).num_days();
            let weeks_diff = diff_days / 7;
            if weeks_diff % interval != 0 {
                return false;
            }

            if byday.is_empty() {
                target_date.weekday() == start_date.weekday()
            } else {
                byday.contains(&target_date.weekday())
            }
        }
        "MONTHLY" => {
            // Repite el mismo día del mes
            target_date.day() == start_date.day()
        }
        "YEARLY" => {
            // Repite el mismo mes y día (cumpleaños, festivos)
            target_date.month() == start_date.month() && target_date.day() == start_date.day()
        }
        _ => false,
    }
}

fn clean_ical_text(text: &str) -> String {
    text.replace("\\,", ",")
        .replace("\\;", ";")
        .replace("\\n", " ")
        .replace("\\N", " ")
        .replace("\\\\", "\\")
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
        assert_eq!(events[0].description, Some("Drink water and stretch".to_string()));
        assert_eq!(events[0].time_label(crate::config::TimeFormat::H12), "06:00 AM - 07:45 AM");
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
}

