use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[derive(Clone)]
pub struct InputController {
    exit_requested: Arc<AtomicBool>,
    armed: Arc<AtomicBool>,
    any_touch_exits: Arc<AtomicBool>,
}

impl InputController {
    pub fn new() -> Self {
        let exit_requested = Arc::new(AtomicBool::new(false));
        // Inicia desarmado para ignorar el toque residual de apertura en KPM
        let armed = Arc::new(AtomicBool::new(false));
        let any_touch_exits = Arc::new(AtomicBool::new(false));

        // 1. Escucha para PC (terminal stdin)
        let exit_pc = Arc::clone(&exit_requested);
        thread::spawn(move || {
            let mut buffer = String::new();
            while !exit_pc.load(Ordering::SeqCst) {
                buffer.clear();
                match std::io::stdin().read_line(&mut buffer) {
                    Ok(0) => break,
                    Ok(_) => {
                        let trimmed = buffer.trim().to_lowercase();
                        if trimmed == "q" || trimmed == "exit" || trimmed == "salir" {
                            exit_pc.store(true, Ordering::SeqCst);
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        // 2. Escucha en todos los dispositivos de entrada de Kindle (/dev/input/event*)
        listen_kindle_all_inputs(
            Arc::clone(&exit_requested),
            Arc::clone(&armed),
            Arc::clone(&any_touch_exits),
        );

        Self {
            exit_requested,
            armed,
            any_touch_exits,
        }
    }

    /// Activa la detección táctil después de haber dibujado la pantalla,
    /// descartando cualquier toque residual previo (ej. al abrir desde KPM).
    pub fn arm(&self, allow_any_touch: bool) {
        thread::sleep(Duration::from_millis(400));
        self.any_touch_exits.store(allow_any_touch, Ordering::SeqCst);
        self.exit_requested.store(false, Ordering::SeqCst);
        self.armed.store(true, Ordering::SeqCst);
    }

    pub fn disarm(&self) {
        self.armed.store(false, Ordering::SeqCst);
    }

    pub fn is_exit_requested(&self) -> bool {
        self.exit_requested.load(Ordering::SeqCst)
    }

    #[allow(dead_code)]
    pub fn request_exit(&self) {
        self.exit_requested.store(true, Ordering::SeqCst);
    }

    /// Espera un toque válido o tecla para salir durante `timeout_secs` segundos
    pub fn wait_for_exit_tap_or_timeout(&self, timeout_secs: u64) -> bool {
        let start = std::time::Instant::now();
        while start.elapsed().as_secs() < timeout_secs {
            if self.is_exit_requested() {
                return true;
            }
            thread::sleep(Duration::from_millis(100));
        }
        self.is_exit_requested()
    }
}

fn discover_event_devices() -> Vec<String> {
    let mut devices = Vec::new();
    if let Ok(entries) = fs::read_dir("/dev/input") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("event") {
                devices.push(entry.path().to_string_lossy().to_string());
            }
        }
    }
    if devices.is_empty() {
        for idx in 0..=12 {
            let dev_path = format!("/dev/input/event{}", idx);
            if Path::new(&dev_path).exists() {
                devices.push(dev_path);
            }
        }
    }
    devices
}

fn listen_kindle_all_inputs(
    exit_flag: Arc<AtomicBool>,
    armed_flag: Arc<AtomicBool>,
    any_touch_flag: Arc<AtomicBool>,
) {
    let (screen_w, screen_h) = crate::platform::get_screen_dimensions();

    for dev_path in discover_event_devices() {
        if let Ok(mut file) = File::open(&dev_path) {
            let flag = Arc::clone(&exit_flag);
            let armed = Arc::clone(&armed_flag);
            let any_touch = Arc::clone(&any_touch_flag);

            thread::spawn(move || {
                // En Linux 32-bit (Kindle ARMv7) input_event es 16 bytes; en 64-bit es 24 bytes
                let is_64 = std::mem::size_of::<usize>() == 8;
                let mut buf = [0u8; 24];
                let packet_size = if is_64 { 24 } else { 16 };
                let type_offset = if is_64 { 16 } else { 8 };

                let mut current_x: i32 = -1;
                let mut current_y: i32 = -1;
                let mut touch_down_in_frame = false;

                loop {
                    if file.read_exact(&mut buf[..packet_size]).is_ok() {
                        let ev_type = u16::from_ne_bytes([buf[type_offset], buf[type_offset + 1]]);
                        let ev_code = u16::from_ne_bytes([buf[type_offset + 2], buf[type_offset + 3]]);
                        let ev_value = i32::from_ne_bytes([
                            buf[type_offset + 4],
                            buf[type_offset + 5],
                            buf[type_offset + 6],
                            buf[type_offset + 7],
                        ]);

                        // Actualizar siempre las últimas coordenadas X / Y reportadas por el panel táctil
                        if ev_type == 3 {
                            match ev_code {
                                // ABS_X (0) o ABS_MT_POSITION_X (53)
                                0 | 53 => {
                                    current_x = ev_value;
                                    if armed.load(Ordering::SeqCst) {
                                        touch_down_in_frame = true;
                                    }
                                }
                                // ABS_Y (1) o ABS_MT_POSITION_Y (54)
                                1 | 54 => {
                                    current_y = ev_value;
                                    if armed.load(Ordering::SeqCst) {
                                        touch_down_in_frame = true;
                                    }
                                }
                                // ABS_MT_TRACKING_ID (57): >= 0 indica inicio/continuación de toque
                                57 => {
                                    if ev_value >= 0 && armed.load(Ordering::SeqCst) {
                                        touch_down_in_frame = true;
                                    } else if ev_value < 0 {
                                        touch_down_in_frame = false;
                                    }
                                }
                                _ => {}
                            }
                        }

                        if !armed.load(Ordering::SeqCst) {
                            touch_down_in_frame = false;
                            continue;
                        }

                        // EV_KEY = 1 (botones físicos o contacto táctil)
                        if ev_type == 1 && ev_value == 1 {
                            // Botón físico de encendido (KEY_POWER = 116) o inicio (KEY_HOME = 102)
                            if ev_code == 116 || ev_code == 102 {
                                flag.store(true, Ordering::SeqCst);
                                break;
                            }
                            // BTN_TOUCH = 330 (toque en pantalla; las coordenadas se evalúan en EV_SYN)
                            if ev_code == 330 {
                                touch_down_in_frame = true;
                            }
                        }

                        // EV_SYN = 0, SYN_REPORT = 0 (fin de trama: current_x y current_y ya están actualizados)
                        if ev_type == 0 && ev_code == 0 && touch_down_in_frame {
                            touch_down_in_frame = false;
                            if is_touch_inside_exit_button(
                                current_x,
                                current_y,
                                screen_w,
                                screen_h,
                                any_touch.load(Ordering::SeqCst),
                            ) {
                                flag.store(true, Ordering::SeqCst);
                                break;
                            }
                        }
                    } else {
                        thread::sleep(Duration::from_millis(50));
                    }
                }
            });
        }
    }
}

/// Verifica si el toque ocurrió específicamente sobre el botón `[ SALIR ]`
/// ubicado en la esquina superior derecha del dashboard (o en cualquier parte si es la pantalla de error).
pub fn is_touch_inside_exit_button(
    x: i32,
    y: i32,
    screen_w: u32,
    screen_h: u32,
    any_touch_exits: bool,
) -> bool {
    if any_touch_exits {
        return true;
    }

    if x < 0 || y < 0 {
        return false;
    }

    // En dashboard.rs, el botón [SALIR] está en la esquina superior derecha:
    // exit_btn_x = width - (220 + 42) * scale  (~75% del ancho hacia la derecha)
    // exit_btn_y = 60 * scale hasta 148 * scale (dentro del header de 210 * scale)
    // Damos un margen cómodo alrededor del botón en la esquina superior derecha:
    // X: desde el 70% del ancho hasta el borde derecho (100%)
    // Y: desde el borde superior (0) hasta el 17% de la altura (~246px)
    let min_x = (screen_w as f32 * 0.70) as i32;
    let max_y = (screen_h as f32 * 0.17).max(230.0) as i32;

    x >= min_x && y <= max_y
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exit_button_hit_zone() {
        let (w, h) = (1072, 1448);
        // Toque sobre el botón [SALIR] (esquina superior derecha, ej. x=900, y=65) -> SALE
        assert!(is_touch_inside_exit_button(900, 65, w, h, false));
        // Toque en la parte izquierda del header (reloj/fecha, ej. x=200, y=65) -> NO SALE
        assert!(!is_touch_inside_exit_button(200, 65, w, h, false));
        // Toque en el centro de la pantalla (eventos, ej. x=500, y=700) -> NO SALE
        assert!(!is_touch_inside_exit_button(500, 700, w, h, false));
        // En pantalla de error (any_touch_exits = true), cualquier toque sale
        assert!(is_touch_inside_exit_button(500, 700, w, h, true));
    }
}
