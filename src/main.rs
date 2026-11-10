mod calendar;
mod config;
mod platform;
mod ui;

use calendar::fetch_all_calendars;
use chrono::{DateTime, Datelike, Duration, FixedOffset, Timelike, Utc};
use config::{Config, Language};
use platform::{
    get_battery_percent, is_kindle, wait_for_network, InputController, KindleGuard,
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

    loop {
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

        // 4. Sincronización fresca al abrir
        app_log("[INFO] Sincronizando calendarios...");

        let events_result = if wait_for_network(5) {
            fetch_all_calendars(&config, now.date_naive(), &offset)
        } else {
            Err(calendar::CalendarFetchError {
                calendar_name: "Red Wi-Fi".to_string(),
                url: "".to_string(),
                error_message: "No se pudo conectar a la red Wi-Fi. Verifica que el Wi-Fi esté encendido."
                    .to_string(),
            })
        };

        match events_result {
            Ok(events) => {
                app_log(&format!(
                    "[INFO] ¡Sincronización exitosa! {} evento(s) encontrado(s) para hoy.",
                    events.len()
                ));

                let battery = get_battery_percent().or(initial_battery);
                render_dashboard(
                    &mut canvas,
                    &now,
                    &events,
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
                    print_cli_summary(&now, &events, config.time_format);
                }
            }
            Err(fetch_err) => {
                app_log(&format!("[ERROR] Falló la sincronización: {}", fetch_err));
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
                    // En pantalla de error, cualquier toque permite salir
                    input.arm(true);
                    app_log("Esperando toque en pantalla o tecla [q] para salir...");
                    input.wait_for_exit_tap_or_timeout(30);
                }
                break;
            }
        }

        if oneshot_mode {
            app_log("[INFO] Modo --oneshot finalizado con éxito.");
            break;
        }

        // 5. Activar escucha táctil SOLO para el botón [SALIR] (allow_any_touch = false)
        input.arm(false);

        let sleep_secs = calculate_sleep_seconds(&now, config.refresh_minutes);
        app_log(&format!(
            "[INFO] Pantalla activa (toca [SALIR] para cerrar). Próxima actualización en {} minutos ({} segundos)...",
            sleep_secs / 60,
            sleep_secs
        ));

        if input.wait_for_exit_tap_or_timeout(sleep_secs as u64) {
            app_log("[INFO] Toque en [SALIR] detectado. Volviendo al menú de Kindle...");
            break;
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
}
