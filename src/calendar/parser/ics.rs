use super::timezone::{TzObservance, VTimezone, parse_tz_offset};
use chrono::NaiveDateTime;
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub(super) struct RawEvent {
    pub(super) uid: String,
    pub(super) status: String,
    pub(super) dtstart: Option<(String, Option<String>, bool)>, // (raw_val, tzid, is_value_date)
    pub(super) dtend: Option<(String, Option<String>, bool)>,
    pub(super) duration_raw: String,
    pub(super) recurrence_id: Option<(String, Option<String>, bool)>,
    pub(super) summary: String,
    pub(super) description: String,
    pub(super) rrule: String,
    pub(super) rdates: Vec<(String, Option<String>, bool)>,
    pub(super) exdates: Vec<(String, Option<String>, bool)>,
}

/// Extracts `VTIMEZONE` definitions and `VEVENT` records from raw ICS content.
pub(super) fn parse_ics_components(ics_raw: &str) -> (HashMap<String, VTimezone>, Vec<RawEvent>) {
    let unfolded = unfold_ics(ics_raw);

    let mut vtimezones: HashMap<String, VTimezone> = HashMap::new();
    let mut raw_events: Vec<RawEvent> = Vec::new();

    let mut stack: Vec<&str> = Vec::new();
    let mut cur_tzid = String::new();
    let mut cur_tz = VTimezone::default();
    let mut cur_obs_dtstart = String::new();
    let mut cur_obs_from = String::new();
    let mut cur_obs_to = String::new();
    let mut cur_obs_rrule = String::new();
    let mut cur_event = RawEvent::default();

    for raw_line in unfolded.lines() {
        let line = raw_line.trim_end_matches('\r').trim();
        if line.is_empty() {
            continue;
        }

        if let Some(comp) = line.strip_prefix("BEGIN:") {
            let comp = comp.trim();
            match comp {
                "VTIMEZONE" => {
                    cur_tzid.clear();
                    cur_tz = VTimezone::default();
                }
                "STANDARD" | "DAYLIGHT" if stack.last() == Some(&"VTIMEZONE") => {
                    cur_obs_dtstart.clear();
                    cur_obs_from.clear();
                    cur_obs_to.clear();
                    cur_obs_rrule.clear();
                }
                "VEVENT" => {
                    cur_event = RawEvent::default();
                }
                _ => {}
            }
            stack.push(comp);
            continue;
        }

        if let Some(comp) = line.strip_prefix("END:") {
            let comp = comp.trim();
            if stack.last() == Some(&comp) {
                stack.pop();
            } else if let Some(pos) = stack.iter().rposition(|c| *c == comp) {
                stack.truncate(pos);
            }

            match comp {
                "VTIMEZONE" => {
                    if !cur_tzid.is_empty() {
                        vtimezones.insert(cur_tzid.clone(), cur_tz.clone());
                    }
                }
                "STANDARD" | "DAYLIGHT" if stack.last() == Some(&"VTIMEZONE") => {
                    if let (Some(dt), Some(from_off), Some(to_off)) = (
                        NaiveDateTime::parse_from_str(cur_obs_dtstart.trim(), "%Y%m%dT%H%M%S").ok(),
                        parse_tz_offset(&cur_obs_from),
                        parse_tz_offset(&cur_obs_to),
                    ) {
                        cur_tz.observances.push(TzObservance {
                            dtstart: dt,
                            offset_from: from_off,
                            offset_to: to_off,
                            rrule: if cur_obs_rrule.is_empty() {
                                None
                            } else {
                                Some(cur_obs_rrule.clone())
                            },
                        });
                    }
                }
                "VEVENT" => {
                    raw_events.push(std::mem::take(&mut cur_event));
                }
                _ => {}
            }
            continue;
        }

        let Some((name, tzid, is_value_date, val)) = parse_prop_line(line) else {
            continue;
        };

        match stack.last().copied() {
            Some("VTIMEZONE") => {
                if name == "TZID" {
                    cur_tzid = val.trim().trim_matches('"').to_string();
                }
            }
            Some("STANDARD") | Some("DAYLIGHT") => match name {
                "DTSTART" => cur_obs_dtstart = val.trim().to_string(),
                "TZOFFSETFROM" => cur_obs_from = val.trim().to_string(),
                "TZOFFSETTO" => cur_obs_to = val.trim().to_string(),
                "RRULE" => cur_obs_rrule = val.trim().to_string(),
                _ => {}
            },
            Some("VEVENT") => match name {
                "UID" => cur_event.uid = val.trim().to_string(),
                "STATUS" => cur_event.status = val.trim().to_uppercase(),
                "DTSTART" => {
                    cur_event.dtstart = Some((val.trim().to_string(), tzid, is_value_date));
                }
                "DTEND" => {
                    cur_event.dtend = Some((val.trim().to_string(), tzid, is_value_date));
                }
                "DURATION" => cur_event.duration_raw = val.trim().to_string(),
                "RECURRENCE-ID" => {
                    cur_event.recurrence_id = Some((val.trim().to_string(), tzid, is_value_date));
                }
                "SUMMARY" => cur_event.summary = val.trim().to_string(),
                "DESCRIPTION" => cur_event.description = val.trim().to_string(),
                "RRULE" => cur_event.rrule = val.trim().to_string(),
                "RDATE" => {
                    for part in val.split(',') {
                        let cleaned = part.trim();
                        if !cleaned.is_empty() {
                            cur_event
                                .rdates
                                .push((cleaned.to_string(), tzid.clone(), is_value_date));
                        }
                    }
                }
                "EXDATE" => {
                    for part in val.split(',') {
                        let cleaned = part.trim();
                        if !cleaned.is_empty() {
                            cur_event
                                .exdates
                                .push((cleaned.to_string(), tzid.clone(), is_value_date));
                        }
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    (vtimezones, raw_events)
}

fn unfold_ics(raw: &str) -> String {
    raw.replace("\r\n", "\n")
        .replace("\n ", "")
        .replace("\n\t", "")
}

fn parse_prop_line(line: &str) -> Option<(&str, Option<String>, bool, &str)> {
    let (left, val) = line.split_once(':')?;
    let mut parts = left.split(';');
    let name = parts.next()?.trim();
    let mut tzid = None;
    let mut is_value_date = false;

    for param in parts {
        if let Some((pk, pv)) = param.split_once('=') {
            let pk_up = pk.trim().to_ascii_uppercase();
            let pv_clean = pv.trim().trim_matches('"');
            if pk_up == "TZID" && !pv_clean.is_empty() {
                tzid = Some(pv_clean.to_string());
            } else if pk_up == "VALUE" && pv_clean.eq_ignore_ascii_case("DATE") {
                is_value_date = true;
            }
        }
    }

    Some((name, tzid, is_value_date, val))
}

pub(super) fn parse_duration_seconds(raw: &str) -> Option<i64> {
    let s = raw.trim();
    let s = s.strip_prefix('+').unwrap_or(s);
    if s.starts_with('-') {
        return None;
    }
    let body = s.strip_prefix('P')?;
    let (date_part, time_part) = match body.split_once('T') {
        Some((d, t)) => (d, Some(t)),
        None => (body, None),
    };

    let mut total_secs: i64 = 0;
    let mut num_buf = String::new();
    for ch in date_part.chars() {
        if ch.is_ascii_digit() {
            num_buf.push(ch);
        } else if !num_buf.is_empty() {
            let n: i64 = num_buf.parse().ok()?;
            num_buf.clear();
            match ch {
                'W' => total_secs += n * 7 * 86400,
                'D' => total_secs += n * 86400,
                _ => return None,
            }
        }
    }

    if let Some(tp) = time_part {
        for ch in tp.chars() {
            if ch.is_ascii_digit() {
                num_buf.push(ch);
            } else if !num_buf.is_empty() {
                let n: i64 = num_buf.parse().ok()?;
                num_buf.clear();
                match ch {
                    'H' => total_secs += n * 3600,
                    'M' => total_secs += n * 60,
                    'S' => total_secs += n,
                    _ => return None,
                }
            }
        }
    }

    Some(total_secs)
}

pub(super) fn clean_ical_text(text: &str) -> String {
    text.replace("\\,", ",")
        .replace("\\;", ";")
        .replace("\\n", " ")
        .replace("\\N", " ")
        .replace("\\\\", "\\")
}
