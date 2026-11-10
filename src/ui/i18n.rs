use crate::config::Language;
use chrono::{DateTime, Datelike, FixedOffset};

#[derive(Debug, Clone, Copy)]
pub struct Translations {
    pub exit_button: &'static str,
    pub all_day_header: &'static str,
    pub all_day_badge: &'static str,
    pub today_agenda_header: &'static str,
    pub no_events_title: &'static str,
    pub no_events_subtitle: &'static str,
    pub footer_hint: &'static str,
    pub pagination_prev_hint: &'static str,
    pub pagination_next_hint: &'static str,
    pub pagination_page_of: fn(usize, usize) -> String,
    pub error_header: &'static str,
    pub error_suggestion_label: &'static str,
    pub error_exit_button: &'static str,
    pub error_exit_sub: &'static str,
    pub format_date: fn(&DateTime<FixedOffset>) -> String,
}

pub static ES: Translations = Translations {
    exit_button: "SALIR",
    all_day_header: "TODO EL DÍA",
    all_day_badge: "TODO EL DÍA",
    today_agenda_header: "AGENDA DE HOY",
    no_events_title: "Sin eventos programados para hoy",
    no_events_subtitle: "Disfruta de tu día libre o añade eventos a tus calendarios.",
    footer_hint: "Toca [SALIR] para volver a Kindle",
    pagination_prev_hint: "‹ Anteriores",
    pagination_next_hint: "Siguientes ›",
    pagination_page_of: |cur, tot| format!("Página {} de {}", cur, tot),
    error_header: "ERROR EN KAUWLENDAR",
    error_suggestion_label: "Sugerencia:",
    error_exit_button: "SALIR AL KINDLE",
    error_exit_sub: "Toca la pantalla para volver al menú de Kindle",
    format_date: format_date_es,
};

pub static EN: Translations = Translations {
    exit_button: "EXIT",
    all_day_header: "ALL DAY",
    all_day_badge: "ALL DAY",
    today_agenda_header: "TODAY'S AGENDA",
    no_events_title: "No events scheduled for today",
    no_events_subtitle: "Enjoy your day off or add events to your calendars.",
    footer_hint: "Tap [EXIT] to return to Kindle",
    pagination_prev_hint: "‹ Previous",
    pagination_next_hint: "Next ›",
    pagination_page_of: |cur, tot| format!("Page {} of {}", cur, tot),
    error_header: "KAUWLENDAR ERROR",
    error_suggestion_label: "Suggestion:",
    error_exit_button: "EXIT TO KINDLE",
    error_exit_sub: "Tap screen to return to Kindle menu",
    format_date: format_date_en,
};

pub fn get_translations(lang: Language) -> &'static Translations {
    match lang {
        Language::Es => &ES,
        Language::En => &EN,
    }
}

fn format_date_es(dt: &DateTime<FixedOffset>) -> String {
    let day_name = match dt.weekday() {
        chrono::Weekday::Mon => "Lunes",
        chrono::Weekday::Tue => "Martes",
        chrono::Weekday::Wed => "Miércoles",
        chrono::Weekday::Thu => "Jueves",
        chrono::Weekday::Fri => "Viernes",
        chrono::Weekday::Sat => "Sábado",
        chrono::Weekday::Sun => "Domingo",
    };

    let month_name = match dt.month() {
        1 => "Enero",
        2 => "Febrero",
        3 => "Marzo",
        4 => "Abril",
        5 => "Mayo",
        6 => "Junio",
        7 => "Julio",
        8 => "Agosto",
        9 => "Septiembre",
        10 => "Octubre",
        11 => "Noviembre",
        12 => "Diciembre",
        _ => "",
    };

    format!("{}, {:02} de {} de {}", day_name, dt.day(), month_name, dt.year())
}

fn format_date_en(dt: &DateTime<FixedOffset>) -> String {
    let day_name = match dt.weekday() {
        chrono::Weekday::Mon => "Monday",
        chrono::Weekday::Tue => "Tuesday",
        chrono::Weekday::Wed => "Wednesday",
        chrono::Weekday::Thu => "Thursday",
        chrono::Weekday::Fri => "Friday",
        chrono::Weekday::Sat => "Saturday",
        chrono::Weekday::Sun => "Sunday",
    };

    let month_name = match dt.month() {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "",
    };

    format!("{}, {} {:02}, {}", day_name, month_name, dt.day(), dt.year())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, NaiveTime};

    #[test]
    fn test_i18n_spanish_formatting() {
        let offset = FixedOffset::east_opt(-4 * 3600).unwrap();
        let dt = NaiveDate::from_ymd_opt(2026, 11, 9)
            .unwrap()
            .and_time(NaiveTime::from_hms_opt(10, 30, 0).unwrap())
            .and_local_timezone(offset)
            .unwrap();

        let i18n = get_translations(Language::Es);
        assert_eq!(i18n.exit_button, "SALIR");
        assert_eq!(i18n.all_day_badge, "TODO EL DÍA");
        assert_eq!((i18n.format_date)(&dt), "Lunes, 09 de Noviembre de 2026");
    }

    #[test]
    fn test_i18n_english_formatting() {
        let offset = FixedOffset::east_opt(-4 * 3600).unwrap();
        let dt = NaiveDate::from_ymd_opt(2026, 11, 9)
            .unwrap()
            .and_time(NaiveTime::from_hms_opt(10, 30, 0).unwrap())
            .and_local_timezone(offset)
            .unwrap();

        let i18n = get_translations(Language::En);
        assert_eq!(i18n.exit_button, "EXIT");
        assert_eq!(i18n.all_day_badge, "ALL DAY");
        assert_eq!((i18n.format_date)(&dt), "Monday, November 09, 2026");
    }
}

