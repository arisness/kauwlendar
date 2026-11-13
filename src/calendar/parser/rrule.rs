use super::timezone::{IcsInstant, VTimezone, parse_ics_instant};
use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDate, Utc, Weekday};
use std::collections::HashMap;

const MAX_RRULE_SCAN_MONTHS: usize = 2400; // 200 years safety cap
const MAX_RRULE_SCAN_STEPS: usize = 200_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Freq {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ByDay {
    ordinal: Option<i32>,
    weekday: Weekday,
}

#[derive(Debug, Clone)]
pub(super) enum UntilBound {
    Date(NaiveDate),
    DateTime(DateTime<Utc>),
}

#[derive(Debug, Clone)]
pub(super) struct RRule {
    pub(super) freq: Freq,
    interval: i64,
    count: Option<usize>,
    pub(super) until: Option<UntilBound>,
    wkst: Weekday,
    byday: Vec<ByDay>,
    bymonthday: Vec<i32>,
    bymonth: Vec<u32>,
    bysetpos: Vec<i32>,
}

fn parse_weekday_code(code: &str) -> Option<Weekday> {
    match code {
        "MO" => Some(Weekday::Mon),
        "TU" => Some(Weekday::Tue),
        "WE" => Some(Weekday::Wed),
        "TH" => Some(Weekday::Thu),
        "FR" => Some(Weekday::Fri),
        "SA" => Some(Weekday::Sat),
        "SU" => Some(Weekday::Sun),
        _ => None,
    }
}

pub(super) fn parse_rrule(
    raw: &str,
    vtimezones: &HashMap<String, VTimezone>,
    local_offset: &FixedOffset,
) -> Option<RRule> {
    let mut freq: Option<Freq> = None;
    let mut interval: i64 = 1;
    let mut count: Option<usize> = None;
    let mut until: Option<UntilBound> = None;
    let mut wkst = Weekday::Mon;
    let mut byday = Vec::new();
    let mut bymonthday = Vec::new();
    let mut bymonth = Vec::new();
    let mut bysetpos = Vec::new();

    for part in raw.split(';') {
        let Some((k, v)) = part.split_once('=') else {
            continue;
        };
        let key = k.trim().to_ascii_uppercase();
        let val = v.trim();
        match key.as_str() {
            "FREQ" => {
                freq = match val.to_ascii_uppercase().as_str() {
                    "DAILY" => Some(Freq::Daily),
                    "WEEKLY" => Some(Freq::Weekly),
                    "MONTHLY" => Some(Freq::Monthly),
                    "YEARLY" => Some(Freq::Yearly),
                    _ => None,
                };
            }
            "INTERVAL" => {
                if let Ok(n) = val.parse::<i64>() {
                    interval = n.max(1);
                }
            }
            "COUNT" => {
                if let Ok(n) = val.parse::<usize>() {
                    count = Some(n);
                }
            }
            "UNTIL" => {
                if let Some(inst) = parse_ics_instant(val, None, false, vtimezones, local_offset) {
                    until = Some(match inst {
                        IcsInstant::AllDay(d) => UntilBound::Date(d),
                        IcsInstant::Timed { utc, .. } => UntilBound::DateTime(utc),
                    });
                }
            }
            "WKST" => {
                if let Some(wd) = parse_weekday_code(&val.to_ascii_uppercase()) {
                    wkst = wd;
                }
            }
            "BYDAY" => {
                for item in val.split(',') {
                    let s = item.trim().to_ascii_uppercase();
                    if s.len() < 2 {
                        continue;
                    }
                    let (ord_str, wd_str) = s.split_at(s.len() - 2);
                    if let Some(wd) = parse_weekday_code(wd_str) {
                        let ordinal = if ord_str.is_empty() {
                            None
                        } else {
                            ord_str.parse::<i32>().ok()
                        };
                        byday.push(ByDay {
                            ordinal,
                            weekday: wd,
                        });
                    }
                }
            }
            "BYMONTHDAY" => {
                for item in val.split(',') {
                    if let Ok(d) = item.trim().parse::<i32>() {
                        if d != 0 {
                            bymonthday.push(d);
                        }
                    }
                }
            }
            "BYMONTH" => {
                for item in val.split(',') {
                    if let Ok(m) = item.trim().parse::<u32>() {
                        if (1..=12).contains(&m) {
                            bymonth.push(m);
                        }
                    }
                }
            }
            "BYSETPOS" => {
                for item in val.split(',') {
                    if let Ok(p) = item.trim().parse::<i32>() {
                        if p != 0 {
                            bysetpos.push(p);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    Some(RRule {
        freq: freq?,
        interval,
        count,
        until,
        wkst,
        byday,
        bymonthday,
        bymonth,
        bysetpos,
    })
}

pub(super) fn within_until(
    date: NaiveDate,
    occ_utc: Option<DateTime<Utc>>,
    until: &UntilBound,
) -> bool {
    match until {
        UntilBound::Date(u_date) => date <= *u_date,
        UntilBound::DateTime(u_dt) => match occ_utc {
            Some(dt) => dt <= *u_dt,
            None => date <= u_dt.date_naive(),
        },
    }
}

pub(super) fn matches_rrule_date(
    start_date: NaiveDate,
    target_occ_utc: Option<DateTime<Utc>>,
    target_date: NaiveDate,
    rule: &RRule,
) -> bool {
    if target_date < start_date {
        return false;
    }
    if let Some(0) = rule.count {
        return false;
    }
    if let Some(until) = &rule.until {
        if !within_until(target_date, target_occ_utc, until) {
            return false;
        }
    }

    // Fast O(1) paths when COUNT is not set
    if rule.count.is_none() {
        return matches_period_on_date(start_date, target_date, rule);
    }

    let max_count = rule.count.unwrap();

    // Fast O(1) path for simple DAILY COUNT (no BY* filters)
    if rule.freq == Freq::Daily
        && rule.byday.is_empty()
        && rule.bymonthday.is_empty()
        && rule.bymonth.is_empty()
        && rule.bysetpos.is_empty()
    {
        let diff = (target_date - start_date).num_days();
        return diff >= 0
            && diff % rule.interval == 0
            && ((diff / rule.interval) as usize) < max_count;
    }

    enumerate_with_count(start_date, target_date, rule, max_count)
}

fn week_start(date: NaiveDate, wkst: Weekday) -> NaiveDate {
    let diff = (date.weekday().num_days_from_monday() as i64
        + 7
        - wkst.num_days_from_monday() as i64)
        % 7;
    date - Duration::days(diff)
}

fn matches_period_on_date(start_date: NaiveDate, target_date: NaiveDate, rule: &RRule) -> bool {
    match rule.freq {
        Freq::Daily => {
            let diff = (target_date - start_date).num_days();
            if diff < 0 || diff % rule.interval != 0 {
                return false;
            }
            if target_date == start_date {
                return true;
            }
            matches_day_filters(target_date, rule)
        }
        Freq::Weekly => {
            let ws_start = week_start(start_date, rule.wkst);
            let ws_target = week_start(target_date, rule.wkst);
            let weeks = (ws_target - ws_start).num_days() / 7;
            if weeks < 0 || weeks % rule.interval != 0 {
                return false;
            }
            if !rule.bymonth.is_empty() && !rule.bymonth.contains(&target_date.month()) {
                return false;
            }
            if !rule.bymonthday.is_empty() && !matches_bymonthday(target_date, &rule.bymonthday) {
                return false;
            }
            if target_date == start_date {
                return true;
            }
            if rule.byday.is_empty() {
                target_date.weekday() == start_date.weekday()
            } else {
                rule.byday.iter().any(|bd| bd.weekday == target_date.weekday())
            }
        }
        Freq::Monthly => {
            let months_diff = (target_date.year() - start_date.year()) * 12
                + (target_date.month() as i32 - start_date.month() as i32);
            if months_diff < 0 || (months_diff as i64) % rule.interval != 0 {
                return false;
            }
            if !rule.bymonth.is_empty() && !rule.bymonth.contains(&target_date.month()) {
                return false;
            }
            if target_date == start_date {
                return true;
            }
            matching_dates_in_month(start_date, target_date.year(), target_date.month(), rule)
                .contains(&target_date)
        }
        Freq::Yearly => {
            let years_diff = target_date.year() - start_date.year();
            if years_diff < 0 || (years_diff as i64) % rule.interval != 0 {
                return false;
            }
            if target_date == start_date {
                return true;
            }
            if !rule.bymonth.is_empty() {
                if !rule.bymonth.contains(&target_date.month()) {
                    return false;
                }
                return matching_dates_in_month(
                    start_date,
                    target_date.year(),
                    target_date.month(),
                    rule,
                )
                .contains(&target_date);
            }
            if !rule.byday.is_empty() && rule.byday.iter().any(|b| b.ordinal.is_some()) {
                // Ordinal BYDAY without BYMONTH targets ordinal within the year
                return matching_dates_in_year(start_date, target_date.year(), rule)
                    .contains(&target_date);
            }
            if target_date.month() != start_date.month() {
                return false;
            }
            matching_dates_in_month(start_date, target_date.year(), target_date.month(), rule)
                .contains(&target_date)
        }
    }
}

fn enumerate_with_count(
    start_date: NaiveDate,
    target_date: NaiveDate,
    rule: &RRule,
    max_count: usize,
) -> bool {
    let mut generated: usize = 0;

    match rule.freq {
        Freq::Daily => {
            let mut cur = start_date;
            for _ in 0..MAX_RRULE_SCAN_STEPS {
                if cur > target_date {
                    return false;
                }
                if cur == start_date || matches_day_filters(cur, rule) {
                    generated += 1;
                    if cur == target_date {
                        return generated <= max_count;
                    }
                    if generated >= max_count {
                        return false;
                    }
                }
                cur += Duration::days(rule.interval);
            }
            false
        }
        Freq::Weekly => {
            let ws0 = week_start(start_date, rule.wkst);
            let mut ws = ws0;
            for _ in 0..MAX_RRULE_SCAN_STEPS {
                if ws > target_date {
                    return false;
                }
                for offset in 0..7 {
                    let d = ws + Duration::days(offset);
                    if d < start_date {
                        continue;
                    }
                    if d > target_date {
                        return false;
                    }
                    let ok_day = if d == start_date {
                        true
                    } else {
                        let ok_wd = if rule.byday.is_empty() {
                            d.weekday() == start_date.weekday()
                        } else {
                            rule.byday.iter().any(|bd| bd.weekday == d.weekday())
                        };
                        let ok_m = rule.bymonth.is_empty() || rule.bymonth.contains(&d.month());
                        let ok_md =
                            rule.bymonthday.is_empty() || matches_bymonthday(d, &rule.bymonthday);
                        ok_wd && ok_m && ok_md
                    };
                    if ok_day {
                        generated += 1;
                        if d == target_date {
                            return generated <= max_count;
                        }
                        if generated >= max_count {
                            return false;
                        }
                    }
                }
                ws += Duration::days(7 * rule.interval);
            }
            false
        }
        Freq::Monthly => {
            let mut year = start_date.year();
            let mut month = start_date.month() as i32;
            for _ in 0..MAX_RRULE_SCAN_MONTHS {
                if year > target_date.year()
                    || (year == target_date.year() && month as u32 > target_date.month())
                {
                    return false;
                }
                let m = month as u32;
                let month_allowed = rule.bymonth.is_empty() || rule.bymonth.contains(&m);
                if month_allowed {
                    let mut dates = matching_dates_in_month(start_date, year, m, rule);
                    if year == start_date.year()
                        && m == start_date.month()
                        && !dates.contains(&start_date)
                    {
                        dates.push(start_date);
                        dates.sort();
                    }
                    for d in dates {
                        if d < start_date {
                            continue;
                        }
                        if d > target_date {
                            return false;
                        }
                        generated += 1;
                        if d == target_date {
                            return generated <= max_count;
                        }
                        if generated >= max_count {
                            return false;
                        }
                    }
                }
                let total_m = (month - 1) + rule.interval as i32;
                year += total_m.div_euclid(12);
                month = total_m.rem_euclid(12) + 1;
            }
            false
        }
        Freq::Yearly => {
            let mut year = start_date.year();
            for _ in 0..MAX_RRULE_SCAN_MONTHS {
                if year > target_date.year() {
                    return false;
                }
                let mut dates = matching_dates_in_year(start_date, year, rule);
                if year == start_date.year() && !dates.contains(&start_date) {
                    dates.push(start_date);
                    dates.sort();
                }
                for d in dates {
                    if d < start_date {
                        continue;
                    }
                    if d > target_date {
                        return false;
                    }
                    generated += 1;
                    if d == target_date {
                        return generated <= max_count;
                    }
                    if generated >= max_count {
                        return false;
                    }
                }
                year += rule.interval as i32;
            }
            false
        }
    }
}

fn matches_day_filters(date: NaiveDate, rule: &RRule) -> bool {
    if !rule.bymonth.is_empty() && !rule.bymonth.contains(&date.month()) {
        return false;
    }
    if !rule.bymonthday.is_empty() && !matches_bymonthday(date, &rule.bymonthday) {
        return false;
    }
    if !rule.byday.is_empty() && !rule.byday.iter().any(|bd| bd.weekday == date.weekday()) {
        return false;
    }
    true
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    };
    next.and_then(|d| d.pred_opt())
        .map(|d| d.day())
        .unwrap_or(28)
}

fn matches_bymonthday(date: NaiveDate, spec: &[i32]) -> bool {
    let dim = days_in_month(date.year(), date.month()) as i32;
    let day = date.day() as i32;
    spec.iter().any(|&v| {
        if v > 0 {
            day == v
        } else {
            day == (dim + 1 + v)
        }
    })
}

fn apply_bysetpos(dates: Vec<NaiveDate>, bysetpos: &[i32]) -> Vec<NaiveDate> {
    if bysetpos.is_empty() || dates.is_empty() {
        return dates;
    }
    let len = dates.len() as i32;
    let mut out = Vec::new();
    for &pos in bysetpos {
        let idx = if pos > 0 { pos - 1 } else { len + pos };
        if idx >= 0 && idx < len {
            let d = dates[idx as usize];
            if !out.contains(&d) {
                out.push(d);
            }
        }
    }
    out.sort();
    out
}

pub(super) fn matching_dates_in_month(
    start_date: NaiveDate,
    year: i32,
    month: u32,
    rule: &RRule,
) -> Vec<NaiveDate> {
    let dim = days_in_month(year, month);
    let mut candidates = Vec::new();

    if !rule.byday.is_empty() {
        let byday_dates = resolve_byday_in_range(
            NaiveDate::from_ymd_opt(year, month, 1).unwrap(),
            NaiveDate::from_ymd_opt(year, month, dim).unwrap(),
            &rule.byday,
        );
        for d in byday_dates {
            if rule.bymonthday.is_empty() || matches_bymonthday(d, &rule.bymonthday) {
                candidates.push(d);
            }
        }
    } else if !rule.bymonthday.is_empty() {
        for day in 1..=dim {
            if let Some(d) = NaiveDate::from_ymd_opt(year, month, day) {
                if matches_bymonthday(d, &rule.bymonthday) {
                    candidates.push(d);
                }
            }
        }
    } else if let Some(d) = NaiveDate::from_ymd_opt(year, month, start_date.day()) {
        candidates.push(d);
    }

    apply_bysetpos(candidates, &rule.bysetpos)
}

pub(super) fn candidate_months_for_yearly(start_date: NaiveDate, rule: &RRule) -> Option<Vec<u32>> {
    if !rule.bymonth.is_empty() {
        let mut ms = rule.bymonth.clone();
        ms.sort();
        ms.dedup();
        Some(ms)
    } else if !rule.byday.is_empty() && rule.byday.iter().any(|b| b.ordinal.is_some()) {
        None
    } else {
        Some(vec![start_date.month()])
    }
}

fn matching_dates_in_year(start_date: NaiveDate, year: i32, rule: &RRule) -> Vec<NaiveDate> {
    if let Some(months) = candidate_months_for_yearly(start_date, rule) {
        let mut all = Vec::new();
        for m in months {
            all.extend(matching_dates_in_month(start_date, year, m, rule));
        }
        all.sort();
        all.dedup();
        return apply_bysetpos(all, &rule.bysetpos);
    }

    let first = NaiveDate::from_ymd_opt(year, 1, 1).unwrap();
    let last = NaiveDate::from_ymd_opt(year, 12, 31).unwrap();
    let mut candidates = resolve_byday_in_range(first, last, &rule.byday);
    if !rule.bymonthday.is_empty() {
        candidates.retain(|d| matches_bymonthday(*d, &rule.bymonthday));
    }
    apply_bysetpos(candidates, &rule.bysetpos)
}

fn resolve_byday_in_range(
    range_start: NaiveDate,
    range_end: NaiveDate,
    byday: &[ByDay],
) -> Vec<NaiveDate> {
    let mut results = Vec::new();
    let mut by_wd: HashMap<Weekday, Vec<NaiveDate>> = HashMap::new();

    let mut d = range_start;
    while d <= range_end {
        by_wd.entry(d.weekday()).or_default().push(d);
        let Some(next) = d.succ_opt() else { break };
        d = next;
    }

    for bd in byday {
        let Some(list) = by_wd.get(&bd.weekday) else {
            continue;
        };
        match bd.ordinal {
            None => results.extend_from_slice(list),
            Some(ord) if ord > 0 => {
                if let Some(&picked) = list.get((ord - 1) as usize) {
                    results.push(picked);
                }
            }
            Some(ord) if ord < 0 => {
                let idx = list.len() as i32 + ord;
                if idx >= 0 {
                    if let Some(&picked) = list.get(idx as usize) {
                        results.push(picked);
                    }
                }
            }
            Some(_) => {}
        }
    }

    results.sort();
    results.dedup();
    results
}
