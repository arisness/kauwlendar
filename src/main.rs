mod calendar;
mod config;
mod platform;
mod ui;

use calendar::{calculate_initial_start_index, fetch_all_calendars};
use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDate, Timelike, Utc};
use config::{Config, Language};
use platform::{
    get_battery_percent, is_kindle, set_wifi, wait_for_network, InputController, KindleGuard,
    UserAction,
};
use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::panic;
use std::process::{Command, Stdio};
use ui::screens::{render_dashboard, render_error_screen};
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
        let now = Utc::now().with_timezone(&offset);
        let today = now.date_naive();

        // 4. Sincronización inteligente de Wi-Fi:
        // Solo conecta al ENTRAR a la app o a partir de las 6:00 AM si cambió de día.
        let is_first_run = last_sync_date.is_none();
        let is_morning_update = last_sync_date != Some(today) && now.hour() >= 6;
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
        render_dashboard(
            &mut canvas,
            &now,
            &cached_events,
            current_start,
            config.time_format,
            battery,
            config.language,
        );

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

        // 5. Activar escucha táctil:
        // - Esquina superior derecha: Botón [SALIR]
        // - Debajo de la cabecera (mitad izq / der): Paginación de eventos (sin botones)
        input.arm(false);

        let sleep_secs = calculate_sleep_seconds(&now, config.refresh_minutes);
        app_log(&format!(
            "[INFO] Pantalla activa (toca [SALIR] para cerrar). Próxima actualización en {} minutos ({} segundos)...",
            sleep_secs / 60,
            sleep_secs
        ));

        let start_wait = std::time::Instant::now();
        let total_sleep = sleep_secs as u64;

        loop {
            let elapsed = start_wait.elapsed().as_secs();
            if elapsed >= total_sleep {
                // Timeout cumplido: actualizar el índice activo para centrar el evento actual
                let cur_mins = (now.hour() * 60 + now.minute()) as i32;
                current_start = calculate_initial_start_index(&cached_events, cur_mins);
                break;
            }

            let remaining = total_sleep - elapsed;
            match input.wait_for_action_or_timeout(remaining) {
                Some(UserAction::Exit) => {
                    app_log("[INFO] Toque en [SALIR] detectado. Volviendo al menú de Kindle...");
                    break 'main_loop;
                }
                Some(UserAction::PagePrev) => {
                    if current_start > 0 {
                        current_start = current_start.saturating_sub(4);
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
                    // Timeout cumplido
                    let cur_mins = (now.hour() * 60 + now.minute()) as i32;
                    current_start = calculate_initial_start_index(&cached_events, cur_mins);
                    break;
                }
            }
        }
    }

    guard.restore();
    app_log("[INFO] Kauwlendar finalizó correctamente.");
}

fn calculate_sleep_seconds(now: &DateTime<FixedOffset>, refresh_mins: u32) -> i64 {
    let hour = now.hour();

    // Modo nocturno: de 21:00 (9 PM) a 06:00 AM
    if hour >= 21 {
        let tomorrow = now.date_naive() + Duration::days(1);
        if let Some(target_naive) = tomorrow.and_hms_opt(6, 0, 0) {
            if let Some(target) = target_naive.and_local_timezone(*now.offset()).single() {
                return (target - *now).num_seconds().max(60);
            }
        }
        return 9 * 3600;
    }

    if hour < 6 {
        if let Some(target_naive) = now.date_naive().and_hms_opt(6, 0, 0) {
            if let Some(target) = target_naive.and_local_timezone(*now.offset()).single() {
                return (target - *now).num_seconds().max(60);
            }
        }
    }

    // Horas diurnas: alinear al siguiente slot
    let step = refresh_mins.max(5) as i64 * 60;
    let secs_today = (now.hour() * 3600 + now.minute() * 60 + now.second()) as i64;
    let next_slot = ((secs_today / step) + 1) * step;
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
