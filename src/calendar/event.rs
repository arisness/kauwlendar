use crate::config::TimeFormat;
use std::cmp::Ordering;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarEvent {
    pub calendar_name: String,
    pub summary: String,
    pub description: Option<String>,
    pub is_all_day: bool,
    pub start_minutes: i32, // -1 para todo el día, 0..1439 para hora
    pub end_minutes: Option<i32>,
}

impl CalendarEvent {
    pub fn new_all_day(calendar_name: impl Into<String>, summary: impl Into<String>) -> Self {
        Self {
            calendar_name: calendar_name.into(),
            summary: summary.into(),
            description: None,
            is_all_day: true,
            start_minutes: -1,
            end_minutes: None,
        }
    }

    pub fn new_timed(
        calendar_name: impl Into<String>,
        summary: impl Into<String>,
        minutes: i32,
    ) -> Self {
        Self {
            calendar_name: calendar_name.into(),
            summary: summary.into(),
            description: None,
            is_all_day: false,
            start_minutes: minutes.clamp(0, 1439),
            end_minutes: None,
        }
    }

    pub fn with_end_minutes(mut self, minutes: i32) -> Self {
        self.end_minutes = Some(minutes.clamp(0, 1439));
        self
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        let text = desc.into();
        if !text.trim().is_empty() {
            self.description = Some(text);
        }
        self
    }

    pub fn time_label(&self, format: TimeFormat) -> String {
        if self.is_all_day {
            return "Todo el día".to_string();
        }

        let start_str = format_minutes(self.start_minutes, format);
        if let Some(end_m) = self.end_minutes {
            if end_m > self.start_minutes {
                let end_str = format_minutes(end_m, format);
                return format!("{} - {}", start_str, end_str);
            }
        }
        start_str
    }
}

fn format_minutes(minutes: i32, format: TimeFormat) -> String {
    let hour = (minutes.clamp(0, 1439) / 60) as u32;
    let minute = (minutes.clamp(0, 1439) % 60) as u32;

    match format {
        TimeFormat::H24 => format!("{:02}:{:02}", hour, minute),
        TimeFormat::H12 => {
            let (h12, period) = match hour {
                0 => (12, "AM"),
                1..=11 => (hour, "AM"),
                12 => (12, "PM"),
                _ => (hour - 12, "PM"),
            };
            format!("{:02}:{:02} {}", h12, minute, period)
        }
    }
}

impl Ord for CalendarEvent {
    fn cmp(&self, other: &Self) -> Ordering {
        // Todo el día primero (-1)
        match (self.is_all_day, other.is_all_day) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => self
                .start_minutes
                .cmp(&other.start_minutes)
                .then_with(|| self.calendar_name.cmp(&other.calendar_name))
                .then_with(|| self.summary.cmp(&other.summary)),
        }
    }
}

impl PartialOrd for CalendarEvent {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_label_formatting() {
        let ev = CalendarEvent::new_timed("Test", "Meet", 14 * 60 + 30); // 14:30
        assert_eq!(ev.time_label(TimeFormat::H24), "14:30");
        assert_eq!(ev.time_label(TimeFormat::H12), "02:30 PM");

        let morning_ev = CalendarEvent::new_timed("Test", "Breakfast", 8 * 60 + 5); // 08:05
        assert_eq!(morning_ev.time_label(TimeFormat::H24), "08:05");
        assert_eq!(morning_ev.time_label(TimeFormat::H12), "08:05 AM");

        let range_ev = CalendarEvent::new_timed("Test", "Work", 6 * 60).with_end_minutes(7 * 60 + 45);
        assert_eq!(range_ev.time_label(TimeFormat::H24), "06:00 - 07:45");
        assert_eq!(range_ev.time_label(TimeFormat::H12), "06:00 AM - 07:45 AM");

        let midnight_ev = CalendarEvent::new_timed("Test", "Night", 15); // 00:15
        assert_eq!(midnight_ev.time_label(TimeFormat::H12), "12:15 AM");

        let all_day = CalendarEvent::new_all_day("Holidays", "Navidad");
        assert_eq!(all_day.time_label(TimeFormat::H12), "Todo el día");
    }

    #[test]
    fn test_event_sorting() {
        let all_day = CalendarEvent::new_all_day("Personal", "Holiday");
        let morning = CalendarEvent::new_timed("Work", "Standup", 9 * 60);
        let afternoon = CalendarEvent::new_timed("Work", "Sync", 15 * 60);

        let mut list = vec![afternoon.clone(), all_day.clone(), morning.clone()];
        list.sort();

        assert_eq!(list, vec![all_day, morning, afternoon]);
    }
}

