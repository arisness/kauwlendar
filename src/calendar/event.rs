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

/// Encuentra el índice del evento en curso o el próximo a ocurrir.
/// Si todos ya pasaron, retorna el índice que permite mostrar los últimos 4 eventos.
pub fn find_active_event_index(events: &[CalendarEvent], current_minutes: i32) -> usize {
    if events.is_empty() {
        return 0;
    }

    // 1. Buscar evento con hora que esté actualmente en curso
    for (i, ev) in events.iter().enumerate() {
        if !ev.is_all_day {
            let end_m = ev.end_minutes.unwrap_or(ev.start_minutes + 60);
            if ev.start_minutes <= current_minutes && current_minutes < end_m {
                return i;
            }
        }
    }

    // 2. Si ninguno está en curso, buscar el próximo evento futuro
    for (i, ev) in events.iter().enumerate() {
        if !ev.is_all_day && ev.start_minutes > current_minutes {
            return i;
        }
    }

    // 3. Si todos los eventos con hora ya terminaron, enfocarse hacia el final
    events.len().saturating_sub(4)
}

/// Calcula el índice de inicio asegurando que se vean 4 eventos (o menos si el total es menor a 4),
/// colocando el evento activo al principio y sin dejar huecos en blanco al final.
pub fn calculate_initial_start_index(events: &[CalendarEvent], current_minutes: i32) -> usize {
    let total = events.len();
    if total <= 4 {
        0
    } else {
        let active_idx = find_active_event_index(events, current_minutes);
        active_idx.min(total.saturating_sub(4))
    }
}

/// Calcula el número de página actual (1-indexed) y el total de páginas
pub fn calculate_pagination(start_idx: usize, total_events: usize, page_size: usize) -> (usize, usize) {
    if total_events <= page_size {
        return (1, 1);
    }
    let total_pages = (total_events + page_size - 1) / page_size;
    if start_idx + page_size >= total_events {
        (total_pages, total_pages)
    } else {
        let page = (start_idx / page_size) + 1;
        (page.min(total_pages), total_pages)
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

    #[test]
    fn test_active_event_index_detection() {
        let events = vec![
            CalendarEvent::new_timed("Test", "Morning", 9 * 60).with_end_minutes(10 * 60), // 0: 09:00 - 10:00
            CalendarEvent::new_timed("Test", "Noon", 12 * 60).with_end_minutes(13 * 60),    // 1: 12:00 - 13:00
            CalendarEvent::new_timed("Test", "Afternoon", 15 * 60).with_end_minutes(16 * 60), // 2: 15:00 - 16:00
            CalendarEvent::new_timed("Test", "Evening 1", 17 * 60).with_end_minutes(18 * 60), // 3: 17:00 - 18:00
            CalendarEvent::new_timed("Test", "Evening 2", 18 * 60).with_end_minutes(19 * 60), // 4: 18:00 - 19:00
            CalendarEvent::new_timed("Test", "Night", 20 * 60).with_end_minutes(21 * 60),     // 5: 20:00 - 21:00
        ];

        // A las 08:30 (antes del primer evento): próximo es index 0
        assert_eq!(find_active_event_index(&events, 8 * 60 + 30), 0);
        assert_eq!(calculate_initial_start_index(&events, 8 * 60 + 30), 0);

        // A las 09:15 (evento 0 en curso): activo es index 0
        assert_eq!(find_active_event_index(&events, 9 * 60 + 15), 0);
        assert_eq!(calculate_initial_start_index(&events, 9 * 60 + 15), 0);

        // A las 12:30 (evento 1 en curso): activo es index 1
        assert_eq!(find_active_event_index(&events, 12 * 60 + 30), 1);
        assert_eq!(calculate_initial_start_index(&events, 12 * 60 + 30), 1);

        // A las 15:10 (evento 2 en curso): activo es index 2
        // total = 6, 6 - 4 = 2. initial_start = min(2, 2) = 2 (muestra [2, 3, 4, 5])
        assert_eq!(find_active_event_index(&events, 15 * 60 + 10), 2);
        assert_eq!(calculate_initial_start_index(&events, 15 * 60 + 10), 2);

        // A las 19:30 (evento 4 ya terminó, próximo es index 5 a las 20:00):
        // total = 6, 6 - 4 = 2. min(5, 2) = 2. Siempre deja 4 eventos en vista!
        assert_eq!(find_active_event_index(&events, 19 * 60 + 30), 5);
        assert_eq!(calculate_initial_start_index(&events, 19 * 60 + 30), 2);

        // A las 22:00 (todos terminaron): deja los últimos 4
        assert_eq!(find_active_event_index(&events, 22 * 60), 2);
        assert_eq!(calculate_initial_start_index(&events, 22 * 60), 2);
    }

    #[test]
    fn test_pagination_calculation() {
        // <= 4 eventos -> siempre 1 de 1
        assert_eq!(calculate_pagination(0, 3, 4), (1, 1));
        assert_eq!(calculate_pagination(0, 4, 4), (1, 1));

        // 7 eventos (2 páginas)
        assert_eq!(calculate_pagination(0, 7, 4), (1, 2));
        assert_eq!(calculate_pagination(2, 7, 4), (1, 2));
        assert_eq!(calculate_pagination(3, 7, 4), (2, 2));

        // 10 eventos (3 páginas)
        assert_eq!(calculate_pagination(0, 10, 4), (1, 3));
        assert_eq!(calculate_pagination(4, 10, 4), (2, 3));
        assert_eq!(calculate_pagination(6, 10, 4), (3, 3));
    }
}

