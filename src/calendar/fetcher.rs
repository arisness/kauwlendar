use super::event::CalendarEvent;
use super::parser::parse_ics_for_date;
use crate::config::Config;
use chrono::{FixedOffset, NaiveDate};
use std::fmt;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct CalendarFetchError {
    pub calendar_name: String,
    pub url: String,
    pub error_message: String,
}

impl fmt::Display for CalendarFetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Calendario '{}' falló:\n{}\nURL: {}",
            self.calendar_name, self.error_message, self.url
        )
    }
}

pub fn fetch_all_calendars(
    config: &Config,
    today: NaiveDate,
    offset: &FixedOffset,
) -> Result<Vec<CalendarEvent>, CalendarFetchError> {
    let mut all_events = Vec::new();
    let mut last_error: Option<CalendarFetchError> = None;
    let mut successful_fetches = 0;

    for cal in &config.calendars {
        if let Some(file_path) = cal.url.strip_prefix("file://") {
            match std::fs::read_to_string(file_path) {
                Ok(body) => {
                    let mut evs = parse_ics_for_date(&body, &cal.name, today, offset);
                    all_events.append(&mut evs);
                    successful_fetches += 1;
                }
                Err(e) => {
                    last_error = Some(CalendarFetchError {
                        calendar_name: cal.name.clone(),
                        url: cal.url.clone(),
                        error_message: format!("No se pudo leer archivo local {}: {}", file_path, e),
                    });
                }
            }
            continue;
        }

        match ureq::get(&cal.url).timeout(Duration::from_secs(15)).call() {
            Ok(resp) => {
                match resp.into_string() {
                    Ok(body) => {
                        let mut evs = parse_ics_for_date(&body, &cal.name, today, offset);
                        all_events.append(&mut evs);
                        successful_fetches += 1;
                    }
                    Err(e) => {
                        last_error = Some(CalendarFetchError {
                            calendar_name: cal.name.clone(),
                            url: cal.url.clone(),
                            error_message: format!("Respuesta no legible como texto: {}", e),
                        });
                    }
                }
            }
            Err(e) => {
                let err_desc = match e {
                    ureq::Error::Status(code, resp) => {
                        format!("Error HTTP {} {}", code, resp.status_text())
                    }
                    ureq::Error::Transport(transport) => {
                        format!("Error de conexión/TLS: {}", transport)
                    }
                };

                last_error = Some(CalendarFetchError {
                    calendar_name: cal.name.clone(),
                    url: cal.url.clone(),
                    error_message: err_desc,
                });
            }
        }
    }

    // Si no se pudo obtener ningún calendario y hubo errores, reportamos el error
    if successful_fetches == 0 && let Some(err) = last_error {
        return Err(err);
    }

    all_events.sort();
    Ok(all_events)
}
