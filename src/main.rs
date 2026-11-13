mod calendar;
mod config;
mod platform;
mod ui;

use calendar::{calculate_initial_start_index, fetch_all_calendars};
use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDate, Timelike};
use config::{Config, DashboardView, Language, NightSchedule};
use platform::{
    current_utc_now, enter_deep_sleep, get_battery_percent, is_kindle, set_wifi,
    wait_for_network, DeepSleepOutcome, InputController, KindleGuard, UserAction,
};
use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::panic;
use std::process::{Command, Stdio};
use ui::screens::{render_dashboard, render_error_screen, render_timeline_dashboard};
use ui::Canvas;

/// Registra mensajes en consola cuando se ejecuta en PC, y en `/tmp/kauwlendar.log` en Kindle
/// para evitar que el visor de scripts de KPM dibuje logs encima de la mitad inferior de la pantalla.
fn app_log(msg: &str) {
    if is_kindle() {
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open("/tmp/kauwlendar.log")
        {
            let _ = writeln!(file, "{}", msg);
        }
    } else {
        println!("{}", msg);
    }
}

fn main() {
    // 1. Configurar hook de pánico para que NUNCA deje el Kindle congelado
    panic::set_hook(Box::new(|info| {
        let msg = format!("\n[FATAL] Ocurrió un error inesperado (panic):\n{}", info);
        app_log(&msg);
        if is_kindle() {
            for proc_name in &["cvm", "awesome", "mesquite"] {
                let _ = Command::new("killall")
                    .args(["-CONT", proc_name])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
            let _ = Command::new("lipc-set-prop")
                .args(["com.lab126.powerd", "preventScreenSaver", "0"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }));

    let args: Vec<String> = env::args().collect();
    let oneshot_mode = args.iter().any(|a| a == "--oneshot" || a == "-1");

    app_log("============================================================");
    app_log("  Kauwlendar v0.2.0 - Dashboard de Calendario E-Ink");
    app_log(&format!(
        "  Plataforma: {}",
        if is_kindle() { "Kindle" } else { "PC / Desktop" }
    ));
    if oneshot_mode {
        app_log("  Modo: Ejecución única (--oneshot)");
    }
    app_log("============================================================");

    // 2. Leer nivel de batería inicial
    let initial_battery = get_battery_percent();

    // Iniciar guardián de ciclo de vida del Kindle y controlador de entrada
    let mut guard = KindleGuard::new();
    let input = InputController::new();

    let mut canvas = Canvas::default_kindle();
    app_log(&format!(
        "[INFO] Resolución de lienzo detectada: {}x{} px",
        canvas.width, canvas.height
    ));

    let mut last_sync_date: Option<NaiveDate> = None;
    let mut cached_events: Vec<calendar::CalendarEvent> = Vec::new();
    let mut current_start: usize = 0;
    let mut interactive_awake: bool = true;

    'main_loop: loop {
        if input.is_exit_requested() {
            app_log("[INFO] Salida solicitada por el usuario. Cerrando...");
            break;
        }

        input.disarm();

        // 3. Cargar configuración
        let (config, config_path) = match Config::load() {
            Ok(cfg) => cfg,
            Err(e) => {
                app_log(&format!("[ERROR] Falló la configuración: {}", e));
                render_error_screen(
                    &mut canvas,
                    "Error de Configuración",
                    &e.to_string(),
                    "Revisa el archivo de configuración en tu Kindle o en la carpeta local.",
                    Language::Es,
                );
                guard.pause_ui();
                let _ = canvas.render_to_screen();

                if !oneshot_mode {
                    // En pantalla de error, cualquier toque permite salir
                    input.arm(true);
                    app_log("Esperando toque en pantalla o tecla [q] para salir...");
                    input.wait_for_exit_tap_or_timeout(30);
                }
                break;
            }
        };

        app_log(&format!(
            "[INFO] Configuración cargada desde: {}",
            config_path.display()
        ));

        let offset = FixedOffset::east_opt(config.utc_offset_hours * 3600)
            .unwrap_or_else(|| FixedOffset::east_opt(0).unwrap());
        let now = current_utc_now().with_timezone(&offset);
        let today = now.date_naive();
        let night_schedule = config.night_schedule().unwrap_or(None);

        // 4. Sincronización inteligente de Wi-Fi:
        // Solo conecta al ENTRAR a la app o al despertar de la franja nocturna (por defecto 06:00 AM) si cambió de día.
        let now_minutes = now.hour() * 60 + now.minute();
        let morning_sync_minutes = night_schedule.map(|s| s.end_minutes).unwrap_or(6 * 60);
        let is_first_run = last_sync_date.is_none();
        let is_morning_update = last_sync_date != Some(today) && now_minutes >= morning_sync_minutes;
        let should_sync_network = is_first_run || is_morning_update;

        if should_sync_network {
            app_log("[INFO] Asegurando Wi-Fi encendido para sincronizar calendarios...");
            set_wifi(true);

            app_log("[INFO] Sincronizando calendarios desde la red...");
            let events_result = if wait_for_network(15) {
                fetch_all_calendars(&config, today, &offset)
            } else {
                Err(calendar::CalendarFetchError {
                    calendar_name: "Red Wi-Fi".to_string(),
                    url: "".to_string(),
                    error_message: "No se pudo conectar a la red Wi-Fi tras encenderla. Verifica la señal y el router.".to_string(),
                })
            };

            // Apagar siempre el Wi-Fi tras sincronizar para ahorrar batería mientras el dashboard está visible
            app_log("[INFO] Sincronización completada. Apagando Wi-Fi para ahorrar batería...");
            set_wifi(false);

            match events_result {
                Ok(events) => {
                    app_log(&format!(
                        "[INFO] ¡Sincronización exitosa! {} evento(s) encontrado(s) para hoy.",
                        events.len()
                    ));
                    cached_events = events;
                    last_sync_date = Some(today);
                    let cur_mins = (now.hour() * 60 + now.minute()) as i32;
                    current_start = calculate_initial_start_index(&cached_events, cur_mins);
                }
                Err(fetch_err) => {
                    app_log(&format!("[ERROR] Falló la sincronización: {}", fetch_err));
                    if is_first_run {
                        render_error_screen(
                            &mut canvas,
                            "Error de Conexión / Calendario",
                            &fetch_err.error_message,
                            &format!(
                                "Verifica que el Wi-Fi esté activo y que la URL del calendario '{}' sea válida.",
                                fetch_err.calendar_name
                            ),
                            config.language,
                        );
                        guard.pause_ui();
                        let _ = canvas.render_to_screen();

                        if !oneshot_mode {
                            input.arm(true);
                            app_log("Esperando toque en pantalla o tecla [q] para salir...");
                            input.wait_for_exit_tap_or_timeout(30);
                        }
                        break 'main_loop;
                    } else {
                        app_log("[WARN] Manteniendo eventos en memoria previos tras fallo de sincronización.");
                    }
                }
            }
        } else {
            app_log(&format!(
                "[INFO] Actualizando pantalla con eventos en memoria ({} eventos). Wi-Fi apagado.",
                cached_events.len()
            ));
        }

        let battery = get_battery_percent().or(initial_battery);
        match config.view {
            DashboardView::Timeline => {
                render_timeline_dashboard(
                    &mut canvas,
                    &now,
                    &cached_events,
                    config.time_format,
                    battery,
                    config.language,
                );
            }
            DashboardView::Cards => {
                render_dashboard(
                    &mut canvas,
                    &now,
                    &cached_events,
                    current_start,
                    config.time_format,
                    battery,
                    config.language,
                );
            }
        }

        guard.pause_ui();
        if let Err(e) = canvas.render_to_screen() {
            app_log(&format!("[ERROR] Error al renderizar en pantalla: {}", e));
        }

        // Imprimir resumen en consola SOLO en PC (nunca en Kindle para no ensuciar la pantalla)
        if !is_kindle() {
            print_cli_summary(&now, &cached_events, config.time_format);
        }

        if oneshot_mode {
            app_log("[INFO] Modo --oneshot finalizado con éxito.");
            break 'main_loop;
        }

        // 5. Fase de espera inteligente (Ventana inicial de 15s + Suspensión mem con rtc0/wakealarm):
        // - Al recién entrar (o tras paginar), espera 15 segundos activos antes de suspender.
        // - Luego apaga el frontlight y entra en `mem` programando `/sys/class/rtc/rtc0/wakealarm`.
        // - Si vence el temporizador (`refresh_minutes`), despierta solo, actualiza la hora/vista y vuelve a dormir.
        // - Si presionas el botón físico de encendido en cualquier momento, despierta de `mem` y sale de una vez.
        input.arm(false);

        let sleep_secs = calculate_sleep_seconds(&now, config.refresh_minutes, night_schedule);
        let total_sleep = sleep_secs.max(5) as u64;
        let awake_window = if interactive_awake {
            15.min(total_sleep)
        } else {
            2.min(total_sleep)
        };

        app_log(&format!(
            "[INFO] Ventana activa por {}s (luego suspensión mem RTC). Próxima actualización en {} minutos ({} segundos)...",
            awake_window,
            total_sleep / 60,
            total_sleep
        ));

        let start_wait = std::time::Instant::now();
        let mut enter_deep = true;

        loop {
            let elapsed = start_wait.elapsed().as_secs();
            if elapsed >= awake_window {
                break;
            }

            let remaining_awake = awake_window - elapsed;
            match input.wait_for_action_or_timeout(remaining_awake) {
                Some(UserAction::Exit) => {
                    app_log("[INFO] Salida detectada. Volviendo al menú de Kindle...");
                    break 'main_loop;
                }
                Some(UserAction::PagePrev) => {
                    if current_start > 0 {
                        current_start = current_start.saturating_sub(4);
                        interactive_awake = true;
                        enter_deep = false;
                        app_log(&format!(
                            "[INFO] Navegando a eventos anteriores (índice inicio: {})",
                            current_start
                        ));
                        break;
                    } else {
                        app_log("[INFO] Ya estás en los primeros eventos.");
                    }
                }
                Some(UserAction::PageNext) => {
                    let total = cached_events.len();
                    let max_start = total.saturating_sub(4);
                    if current_start < max_start {
                        current_start = (current_start + 4).min(max_start);
                        interactive_awake = true;
                        enter_deep = false;
                        app_log(&format!(
                            "[INFO] Navegando a eventos siguientes (índice inicio: {})",
                            current_start
                        ));
                        break;
                    } else {
                        app_log("[INFO] Ya estás en los últimos eventos.");
                    }
                }
                None => {
                    break;
                }
            }
        }

        if !enter_deep {
            continue 'main_loop;
        }

        let elapsed_awake = start_wait.elapsed().as_secs();
        let deep_sleep_secs = total_sleep.saturating_sub(elapsed_awake);

        if deep_sleep_secs > 0 {
            app_log(&format!(
                "[INFO] Entrando en mem (rtc0/wakealarm) por {} segundos. Pulsa el botón físico para salir.",
                deep_sleep_secs
            ));
            guard.disable_frontlight();

            let sleep_report = enter_deep_sleep(deep_sleep_secs, &input);
            app_log(&format!(
                "[INFO] Retorno de mem: modo={}, sys={}ms, rtc={}ms, salida={:?}",
                sleep_report.mode,
                sleep_report.sys_elapsed_ms,
                sleep_report.rtc_elapsed_ms,
                sleep_report.outcome
            ));
            match sleep_report.outcome {
                DeepSleepOutcome::UserWakeup => {
                    app_log("[INFO] Botón físico detectado. Saliendo de inmediato al Kindle...");
                    break 'main_loop;
                }
                DeepSleepOutcome::TimerExpired => {
                    app_log("[INFO] Alarma RTC cumplida. Actualizando vista del calendario...");
                    interactive_awake = false;
                }
                DeepSleepOutcome::Failed(err_msg) => {
                    app_log(&format!("[ERROR] Falló la suspensión en mem: {}", err_msg));
                    let (title, hint) = match config.language {
                        Language::Es => (
                            "Error de Suspensión (mem)",
                            "Desconecta el cable USB de datos si está conectado al PC e inténtalo de nuevo.",
                        ),
                        Language::En => (
                            "Suspend Error (mem)",
                            "Disconnect the USB data cable if plugged into a PC and try again.",
                        ),
                    };
                    render_error_screen(&mut canvas, title, &err_msg, hint, config.language);
                    guard.pause_ui();
                    let _ = canvas.render_to_screen();
                    input.arm(true);
                    app_log("Esperando toque en pantalla o botón físico para salir...");
                    input.wait_for_exit_tap_or_timeout(30);
                    break 'main_loop;
                }
            }
        } else {
            interactive_awake = false;
        }

        let refreshed_now = current_utc_now().with_timezone(&offset);
        let cur_mins = (refreshed_now.hour() * 60 + refreshed_now.minute()) as i32;
        current_start = calculate_initial_start_index(&cached_events, cur_mins);
    }

    guard.restore();
    app_log("[INFO] Kauwlendar finalizó correctamente.");
}

fn seconds_until_day_time(
    now: &DateTime<FixedOffset>,
    target_date: NaiveDate,
    target_minutes: u32,
) -> Option<i64> {
    let hour = target_minutes / 60;
    let minute = target_minutes % 60;
    let target_naive = target_date.and_hms_opt(hour, minute, 0)?;
    let target = target_naive.and_local_timezone(*now.offset()).single()?;
    Some((target - *now).num_seconds().max(60))
}

fn calculate_sleep_seconds(
    now: &DateTime<FixedOffset>,
    refresh_mins: u32,
    night_schedule: Option<NightSchedule>,
) -> i64 {
    let now_minutes = now.hour() * 60 + now.minute();
    let secs_today = (now.hour() * 3600 + now.minute() * 60 + now.second()) as i64;

    if let Some(sched) = night_schedule {
        if sched.start_minutes > sched.end_minutes {
            // Franja nocturna que cruza medianoche (ej. 21:00 -> 06:00)
            if now_minutes >= sched.start_minutes {
                if let Some(secs) = seconds_until_day_time(
                    now,
                    now.date_naive() + Duration::days(1),
                    sched.end_minutes,
                ) {
                    return secs;
                }
            } else if now_minutes < sched.end_minutes {
                if let Some(secs) = seconds_until_day_time(now, now.date_naive(), sched.end_minutes)
                {
                    return secs;
                }
            }
        } else if now_minutes >= sched.start_minutes && now_minutes < sched.end_minutes {
            // Franja nocturna dentro del mismo día (ej. 01:00 -> 06:00)
            if let Some(secs) = seconds_until_day_time(now, now.date_naive(), sched.end_minutes) {
                return secs;
            }
        }
    }

    // Horas diurnas (o modo 24h): alinear al siguiente slot
    let step = refresh_mins.max(5) as i64 * 60;
    let mut next_slot = ((secs_today / step) + 1) * step;

    if let Some(sched) = night_schedule {
        let start_secs = sched.start_minutes as i64 * 60;
        if secs_today < start_secs && next_slot > start_secs {
            next_slot = start_secs;
        }
    }

    (next_slot - secs_today).max(60)
}

fn print_cli_summary(
    now: &DateTime<FixedOffset>,
    events: &[calendar::CalendarEvent],
    format: config::TimeFormat,
) {
    println!("\n------------------------------------------------------------");
    println!("  AGENDA PARA HOY: {:04}-{:02}-{:02} | Formato: {}", now.year(), now.month(), now.day(), format);
    println!("------------------------------------------------------------");
    if events.is_empty() {
        println!("  (No hay eventos programados)");
    } else {
        for ev in events {
            println!("  [{:>11}]  {:<12} {}", ev.time_label(format), format!("[{}]", ev.calendar_name), ev.summary);
        }
    }
    println!("------------------------------------------------------------\n");
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn dt(y: i32, m: u32, d: u32, hh: u32, mm: u32, ss: u32) -> DateTime<FixedOffset> {
        let offset = FixedOffset::east_opt(-4 * 3600).unwrap();
        offset.with_ymd_and_hms(y, m, d, hh, mm, ss).unwrap()
    }

    #[test]
    fn test_calculate_sleep_seconds_default_cross_midnight() {
        let sched = Some(NightSchedule {
            start_minutes: 21 * 60,
            end_minutes: 6 * 60,
        });
        assert_eq!(calculate_sleep_seconds(&dt(2026, 11, 13, 21, 0, 0), 30, sched), 9 * 3600);
        assert_eq!(calculate_sleep_seconds(&dt(2026, 11, 14, 2, 0, 0), 30, sched), 4 * 3600);
        assert_eq!(calculate_sleep_seconds(&dt(2026, 11, 13, 16, 17, 0), 30, sched), 13 * 60);
    }

    #[test]
    fn test_calculate_sleep_seconds_custom_minutes_and_off() {
        let custom = Some(NightSchedule {
            start_minutes: 22 * 60 + 30,
            end_minutes: 6 * 60 + 45,
        });
        // Clamped at night_start (22:30) instead of overshooting to 23:00 when refresh = 60
        assert_eq!(
            calculate_sleep_seconds(&dt(2026, 11, 13, 22, 0, 0), 60, custom),
            30 * 60
        );
        // Inside custom night window (22:30 -> 06:45 = 8h 15m)
        assert_eq!(
            calculate_sleep_seconds(&dt(2026, 11, 13, 22, 30, 0), 30, custom),
            8 * 3600 + 15 * 60
        );
        // Night mode "off" updates every 30 min 24/7
        assert_eq!(calculate_sleep_seconds(&dt(2026, 11, 13, 23, 30, 0), 30, None), 30 * 60);
        assert_eq!(calculate_sleep_seconds(&dt(2026, 11, 14, 2, 0, 0), 30, None), 30 * 60);
    }

    #[test]
    fn test_calculate_sleep_seconds_same_day_window() {
        let same_day = Some(NightSchedule {
            start_minutes: 1 * 60,
            end_minutes: 5 * 60 + 30,
        });
        assert_eq!(
            calculate_sleep_seconds(&dt(2026, 11, 14, 1, 0, 0), 30, same_day),
            4 * 3600 + 30 * 60
        );
        assert_eq!(
            calculate_sleep_seconds(&dt(2026, 11, 14, 0, 30, 0), 30, same_day),
            30 * 60
        );
    }
}
