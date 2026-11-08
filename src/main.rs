use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDate, NaiveDateTime, Timelike, Utc};
use serde::Deserialize;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration as StdDuration;

const CONFIG_PATH: &str = "/mnt/us/kauwlendar/calendars.toml";
const CACHE_PATH: &str = "/mnt/us/kauwlendar/cached_events.txt";

#[derive(Deserialize, Debug)]
struct Config {
    refresh_minutes: u32,
    utc_offset_hours: i32,
    calendars: Vec<CalendarSource>,
}

#[derive(Deserialize, Debug)]
struct CalendarSource {
    name: String,
    url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CalendarEvent {
    sort_minutes: i32, // -1 para eventos de todo el día, 0..1439 para hora exacta
    time_label: String,
    calendar_name: String,
    summary: String,
}

fn main() {
    loop {
        let config = match load_config() {
            Ok(cfg) => cfg,
            Err(e) => {
                render_screen(&["ERROR DE CONFIGURACION".to_string(), e]);
                return;
            }
        };

        let offset = FixedOffset::east_opt(config.utc_offset_hours * 3600)
            .unwrap_or_else(|| FixedOffset::east_opt(0).unwrap());
        let now = Utc::now().with_timezone(&offset);
        let hour = now.hour();

        // Sincronizar con Wi-Fi a las 6:00 AM o si aún no existe caché local
        let should_sync = (hour == 6 && now.minute() < config.refresh_minutes)
            || !Path::new(CACHE_PATH).exists();

        let events = if should_sync {
            set_wifi(true);
            let fetched = if wait_for_network(20) {
                fetch_all_calendars(&config, now.date_naive(), &offset)
            } else {
                load_cached_events()
            };
            set_wifi(false);
            save_cached_events(&fetched);
            fetched
        } else {
            load_cached_events()
        };

        // Construir las líneas a pintar en la pantalla e-ink
        let mut lines = Vec::new();
        lines.push(format!(
            "=== {:04}-{:02}-{:02} | {:02}:{:02} ===",
            now.year(),
            now.month(),
            now.day(),
            now.hour(),
            now.minute()
        ));
        lines.push(String::from("----------------------------------------"));

        if events.is_empty() {
            lines.push(String::from("Sin eventos programados para hoy."));
        } else {
            for ev in &events {
                lines.push(format!(
                    "{} [{}] {}",
                    ev.time_label, ev.calendar_name, ev.summary
                ));
            }
        }

        // Calcular cuánto tiempo dormir según el horario (6:00 AM a 9:00 PM)
        let sleep_secs = calculate_sleep_seconds(&now, config.refresh_minutes);
        if hour >= 21 || hour < 6 {
            lines.push(String::from("----------------------------------------"));
            lines.push(String::from("[Modo nocturno: despierta a las 06:00]"));
        }

        render_screen(&lines);

        // Suspender el Kindle en RAM hasta el próximo ciclo
        sleep_with_rtcwake(sleep_secs);
    }
}

fn load_config() -> Result<Config, String> {
    let content = fs::read_to_string(CONFIG_PATH)
        .map_err(|e| format!("No se pudo leer {}: {}", CONFIG_PATH, e))?;
    toml::from_str(&content).map_err(|e| format!("TOML invalido: {}", e))
}

fn fetch_all_calendars(
    config: &Config,
    today: NaiveDate,
    offset: &FixedOffset,
) -> Vec<CalendarEvent> {
    let mut all_events = Vec::new();

    for cal in &config.calendars {
        if let Ok(resp) = ureq::get(&cal.url)
            .timeout(StdDuration::from_secs(15))
            .call()
        {
            if let Ok(body) = resp.into_string() {
                let mut evs = parse_ics_for_date(&body, &cal.name, today, offset);
                all_events.append(&mut evs);
            }
        }
    }

    all_events.sort();
    all_events
}

fn parse_ics_for_date(
    ics_raw: &str,
    cal_name: &str,
    target_date: NaiveDate,
    local_offset: &FixedOffset,
) -> Vec<CalendarEvent> {
    // Desplegar líneas continuadas según RFC 5545 (CRLF + espacio/tab)
    let unfolded = ics_raw.replace("\r\n ", "").replace("\n ", "").replace("\r\n\t", "").replace("\n\t", "");
    let mut events = Vec::new();

    let mut in_event = false;
    let mut dtstart_raw = String::new();
    let mut summary = String::new();

    for line in unfolded.lines() {
        let line = line.trim_end_matches('\r');
        if line == "BEGIN:VEVENT" {
            in_event = true;
            dtstart_raw.clear();
            summary.clear();
        } else if line == "END:VEVENT" && in_event {
            in_event = false;
            if let Some((date, sort_min, label)) = parse_ical_dtstart(&dtstart_raw, local_offset) {
                if date == target_date {
                    events.push(CalendarEvent {
                        sort_minutes: sort_min,
                        time_label: label,
                        calendar_name: cal_name.to_string(),
                        summary: clean_ical_text(&summary),
                    });
                }
            }
        } else if in_event {
            if line.starts_with("DTSTART") {
                if let Some((_, val)) = line.split_once(':') {
                    dtstart_raw = val.trim().to_string();
                }
            } else if let Some(val) = line.strip_prefix("SUMMARY:") {
                summary = val.trim().to_string();
            } else if line.starts_with("SUMMARY;") {
                if let Some((_, val)) = line.split_once(':') {
                    summary = val.trim().to_string();
                }
            }
        }
    }

    events
}

fn parse_ical_dtstart(raw: &str, local_offset: &FixedOffset) -> Option<(NaiveDate, i32, String)> {
    // Caso 1: Evento de día completo (YYYYMMDD)
    if raw.len() == 8 {
        let date = NaiveDate::parse_from_str(raw, "%Y%m%d").ok()?;
        return Some((date, -1, String::from("Todo el dia")));
    }

    // Caso 2: Fecha y hora en UTC (YYYYMMDDTHHMMSSZ)
    if raw.ends_with('Z') && raw.len() >= 16 {
        let clean = raw.trim_end_matches('Z');
        let naive = NaiveDateTime::parse_from_str(clean, "%Y%m%dT%H%M%S").ok()?;
        let utc_dt = DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc);
        let local_dt = utc_dt.with_timezone(local_offset);
        let mins = (local_dt.hour() * 60 + local_dt.minute()) as i32;
        let label = format!("{:02}:{:02}", local_dt.hour(), local_dt.minute());
        return Some((local_dt.date_naive(), mins, label));
    }

    // Caso 3: Fecha y hora local (YYYYMMDDTHHMMSS)
    if raw.len() >= 15 {
        let slice = &raw[..15];
        let naive = NaiveDateTime::parse_from_str(slice, "%Y%m%dT%H%M%S").ok()?;
        let mins = (naive.hour() * 60 + naive.minute()) as i32;
        let label = format!("{:02}:{:02}", naive.hour(), naive.minute());
        return Some((naive.date(), mins, label));
    }

    None
}

fn clean_ical_text(text: &str) -> String {
    text.replace("\\,", ",")
        .replace("\\;", ";")
        .replace("\\n", " ")
        .replace("\\\\", "\\")
}

fn save_cached_events(events: &[CalendarEvent]) {
    let mut data = String::new();
    for ev in events {
        data.push_str(&format!(
            "{}|{}|{}|{}\n",
            ev.sort_minutes, ev.time_label, ev.calendar_name, ev.summary
        ));
    }
    let _ = fs::write(CACHE_PATH, data);
}

fn load_cached_events() -> Vec<CalendarEvent> {
    let mut events = Vec::new();
    if let Ok(content) = fs::read_to_string(CACHE_PATH) {
        for line in content.lines() {
            let parts: Vec<&str> = line.splitn(4, '|').collect();
            if parts.len() == 4 {
                if let Ok(sort_minutes) = parts[0].parse::<i32>() {
                    events.push(CalendarEvent {
                        sort_minutes,
                        time_label: parts[1].to_string(),
                        calendar_name: parts[2].to_string(),
                        summary: parts[3].to_string(),
                    });
                }
            }
        }
    }
    events
}

fn calculate_sleep_seconds(now: &DateTime<FixedOffset>, refresh_mins: u32) -> i64 {
    let hour = now.hour();

    // Si son las 9:00 PM (21:00) o más tarde, dormir directo hasta las 6:00 AM de mañana
    if hour >= 21 {
        let tomorrow = now.date_naive() + Duration::days(1);
        if let Some(target_naive) = tomorrow.and_hms_opt(6, 0, 0) {
            if let Some(target) = target_naive.and_local_timezone(*now.offset()).single() {
                return (target - *now).num_seconds().max(60);
            }
        }
        return 9 * 3600;
    }

    // Si despertó de madrugada antes de las 6:00 AM, dormir hasta las 6:00 AM de hoy
    if hour < 6 {
        if let Some(target_naive) = now.date_naive().and_hms_opt(6, 0, 0) {
            if let Some(target) = target_naive.and_local_timezone(*now.offset()).single() {
                return (target - *now).num_seconds().max(60);
            }
        }
    }

    // Entre 6:00 AM y 8:59 PM: alinear al siguiente bloque (ej. :00 o :30)
    let step = refresh_mins.max(1) as i64 * 60;
    let secs_today = (now.hour() * 3600 + now.minute() * 60 + now.second()) as i64;
    let next_slot = ((secs_today / step) + 1) * step;
    (next_slot - secs_today).max(60)
}

fn set_wifi(enable: bool) {
    let val = if enable { "1" } else { "0" };
    let _ = Command::new("lipc-set-prop")
        .args(["com.lab126.cmd", "wirelessEnable", val])
        .status();
}

fn wait_for_network(timeout_secs: u32) -> bool {
    for _ in 0..timeout_secs {
        if let Ok(out) = Command::new("ip").args(["-4", "addr", "show", "wlan0"]).output() {
            let stdout = String::from_utf8_lossy(&out.stdout);
            if stdout.contains("inet ") {
                thread::sleep(StdDuration::from_secs(2)); // Espera breve para DNS/rutas
                return true;
            }
        }
        thread::sleep(StdDuration::from_secs(1));
    }
    false
}

fn render_screen(lines: &[String]) {
    let fbink_path = "/mnt/us/koreader/fbink";
    if Path::new(fbink_path).exists() {
        // Limpiar pantalla con fbink (-c) sin parpadeo agresivo
        let _ = Command::new(fbink_path).args(["-q", "-c"]).status();

        for (idx, line) in lines.iter().enumerate() {
            let row = (idx * 2 + 2).to_string();
            // El "--" evita que fbink confunda lineas que empiezan por "-" con flags
            let _ = Command::new(fbink_path)
                .args(["-q", "-y", &row, "--", line])
                .status();
        }
    } else {
        let _ = Command::new("eips").arg("-c").status();
        for (idx, line) in lines.iter().enumerate() {
            let row = (idx * 2 + 2).to_string();
            let safe_line: String = line
                .chars()
                .map(|c| if c.is_ascii() { c } else { '?' })
                .take(48)
                .collect();
            let _ = Command::new("eips").args(["2", &row, &safe_line]).status();
        }
    }
}

fn sleep_with_rtcwake(seconds: i64) {
    // Habilitar permisos de wakeup en el kernel para rtc0 y rtc1
    let _ = fs::write("/sys/class/rtc/rtc0/device/power/wakeup", "enabled");
    let _ = fs::write("/sys/class/rtc/rtc1/device/power/wakeup", "enabled");

    let sec_str = seconds.to_string();

    // Intentar primero con rtc0, si falla probar con rtc1
    let status_rtc0 = Command::new("rtcwake")
        .args(["-d", "rtc0", "-m", "mem", "-s", &sec_str])
        .status();

    if status_rtc0.map(|s| !s.success()).unwrap_or(true) {
        let status_rtc1 = Command::new("rtcwake")
            .args(["-d", "rtc1", "-m", "mem", "-s", &sec_str])
            .status();

        if status_rtc1.map(|s| !s.success()).unwrap_or(true) {
            thread::sleep(StdDuration::from_secs(seconds as u64));
        }
    }
}