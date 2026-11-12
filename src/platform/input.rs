use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub const ACTION_NONE: u8 = 0;
pub const ACTION_EXIT: u8 = 1;
pub const ACTION_PAGE_PREV: u8 = 2;
pub const ACTION_PAGE_NEXT: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserAction {
    Exit,
    PagePrev,
    PageNext,
}

#[derive(Clone)]
pub struct InputController {
    exit_requested: Arc<AtomicBool>,
    action_flag: Arc<AtomicU8>,
    armed: Arc<AtomicBool>,
    button_armed: Arc<AtomicBool>,
    any_touch_exits: Arc<AtomicBool>,
    notifier: Arc<(Mutex<()>, Condvar)>,
}

impl InputController {
    pub fn new() -> Self {
        let exit_requested = Arc::new(AtomicBool::new(false));
        let action_flag = Arc::new(AtomicU8::new(ACTION_NONE));
        // Inicia desarmado para ignorar el toque residual de apertura en KPM
        let armed = Arc::new(AtomicBool::new(false));
        let button_armed = Arc::new(AtomicBool::new(false));
        let any_touch_exits = Arc::new(AtomicBool::new(false));
        let notifier = Arc::new((Mutex::new(()), Condvar::new()));

        // 1. Escucha para PC (terminal stdin) — solo en entorno de escritorio
        if !crate::platform::is_kindle() {
            let exit_pc = Arc::clone(&exit_requested);
            let action_pc = Arc::clone(&action_flag);
            let notify_pc = Arc::clone(&notifier);
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
                                action_pc.store(ACTION_EXIT, Ordering::SeqCst);
                                notify_waiters(&notify_pc);
                                break;
                            } else if trimmed == "a"
                                || trimmed == "left"
                                || trimmed == "prev"
                                || trimmed == "<"
                            {
                                action_pc.store(ACTION_PAGE_PREV, Ordering::SeqCst);
                                notify_waiters(&notify_pc);
                            } else if trimmed == "d"
                                || trimmed == "right"
                                || trimmed == "next"
                                || trimmed == ">"
                            {
                                action_pc.store(ACTION_PAGE_NEXT, Ordering::SeqCst);
                                notify_waiters(&notify_pc);
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
        }

        // 2. Escucha en los dispositivos de entrada de Kindle (/dev/input/event*)
        listen_kindle_all_inputs(
            Arc::clone(&exit_requested),
            Arc::clone(&action_flag),
            Arc::clone(&armed),
            Arc::clone(&button_armed),
            Arc::clone(&any_touch_exits),
            Arc::clone(&notifier),
        );

        Self {
            exit_requested,
            action_flag,
            armed,
            button_armed,
            any_touch_exits,
            notifier,
        }
    }

    /// Activa la detección táctil y de botón físico después de haber dibujado la pantalla,
    /// descartando cualquier toque residual previo (ej. al abrir desde KPM).
    pub fn arm(&self, allow_any_touch: bool) {
        thread::sleep(Duration::from_millis(400));
        self.any_touch_exits.store(allow_any_touch, Ordering::SeqCst);
        self.exit_requested.store(false, Ordering::SeqCst);
        self.action_flag.store(ACTION_NONE, Ordering::SeqCst);
        self.button_armed.store(true, Ordering::SeqCst);
        self.armed.store(true, Ordering::SeqCst);
    }

    pub fn disarm(&self) {
        self.armed.store(false, Ordering::SeqCst);
        self.button_armed.store(false, Ordering::SeqCst);
    }

    pub fn is_exit_requested(&self) -> bool {
        self.exit_requested.load(Ordering::SeqCst)
            || self.action_flag.load(Ordering::SeqCst) == ACTION_EXIT
    }

    #[allow(dead_code)]
    pub fn request_exit(&self) {
        self.exit_requested.store(true, Ordering::SeqCst);
        self.action_flag.store(ACTION_EXIT, Ordering::SeqCst);
        notify_waiters(&self.notifier);
    }

    pub fn poll_action(&self) -> Option<UserAction> {
        let val = self.action_flag.swap(ACTION_NONE, Ordering::SeqCst);
        match val {
            ACTION_EXIT => Some(UserAction::Exit),
            ACTION_PAGE_PREV => Some(UserAction::PagePrev),
            ACTION_PAGE_NEXT => Some(UserAction::PageNext),
            _ => {
                if self.exit_requested.load(Ordering::SeqCst) {
                    Some(UserAction::Exit)
                } else {
                    None
                }
            }
        }
    }

    /// Espera una acción de usuario (bloqueando el hilo con 0% de uso de CPU mediante Condvar) o timeout
    pub fn wait_for_action_or_timeout(&self, timeout_secs: u64) -> Option<UserAction> {
        if let Some(act) = self.poll_action() {
            return Some(act);
        }
        if timeout_secs == 0 {
            return None;
        }

        let deadline = Instant::now() + Duration::from_secs(timeout_secs);
        let (lock, cvar) = &*self.notifier;
        let mut guard = match lock.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };

        loop {
            if let Some(act) = self.poll_action() {
                return Some(act);
            }
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            let remaining = deadline - now;
            match cvar.wait_timeout(guard, remaining) {
                Ok((g, _)) => guard = g,
                Err(poisoned) => guard = poisoned.into_inner().0,
            }
        }

        self.poll_action()
    }

    /// Espera un toque válido o tecla para salir durante `timeout_secs` segundos sin polling
    pub fn wait_for_exit_tap_or_timeout(&self, timeout_secs: u64) -> bool {
        if self.is_exit_requested() {
            return true;
        }
        if timeout_secs == 0 {
            return false;
        }

        let deadline = Instant::now() + Duration::from_secs(timeout_secs);
        let (lock, cvar) = &*self.notifier;
        let mut guard = match lock.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };

        loop {
            if self.is_exit_requested() {
                return true;
            }
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            let remaining = deadline - now;
            match cvar.wait_timeout(guard, remaining) {
                Ok((g, _)) => guard = g,
                Err(poisoned) => guard = poisoned.into_inner().0,
            }
        }

        self.is_exit_requested()
    }
}

fn notify_waiters(notifier: &Arc<(Mutex<()>, Condvar)>) {
    let (lock, cvar) = &**notifier;
    if let Ok(_guard) = lock.lock() {
        cvar.notify_all();
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
    if devices.empty() {
        for idx in 0..=12 {
            let dev_path = format!("/dev/input/event{}", idx);
            if Path::new(&dev_path).exists() {
                devices.push(dev_path);
            }
        }
    }
    devices
}

trait VecEmptyExt {
    fn empty(&self) -> bool;
}

impl<T> VecEmptyExt for Vec<T> {
    fn empty(&self) -> bool {
        self.is_empty()
    }
}

fn listen_kindle_all_inputs(
    exit_flag: Arc<AtomicBool>,
    action_flag: Arc<AtomicU8>,
    armed_flag: Arc<AtomicBool>,
    button_armed_flag: Arc<AtomicBool>,
    any_touch_flag: Arc<AtomicBool>,
    notifier: Arc<(Mutex<()>, Condvar)>,
) {
    let (screen_w, screen_h) = crate::platform::get_screen_dimensions();

    for dev_path in discover_event_devices() {
        if let Ok(mut file) = File::open(&dev_path) {
            let exit_f = Arc::clone(&exit_flag);
            let action_f = Arc::clone(&action_flag);
            let armed = Arc::clone(&armed_flag);
            let btn_armed = Arc::clone(&button_armed_flag);
            let any_touch = Arc::clone(&any_touch_flag);
            let notify = Arc::clone(&notifier);

            thread::spawn(move || {
                // En Linux 32-bit (Kindle ARMv7) input_event es 16 bytes; en 64-bit es 24 bytes
                let is_64 = std::mem::size_of::<usize>() == 8;
                let mut buf = [0u8; 24];
                let packet_size = if is_64 { 24 } else { 16 };
                let type_offset = if is_64 { 16 } else { 8 };

                let mut current_x: i32 = -1;
                let mut current_y: i32 = -1;
                let mut touch_down_in_frame = false;
                let mut consecutive_errors: u8 = 0;

                loop {
                    if exit_f.load(Ordering::SeqCst) {
                        break;
                    }

                    if file.read_exact(&mut buf[..packet_size]).is_ok() {
                        consecutive_errors = 0;
                        let ev_type = u16::from_ne_bytes([buf[type_offset], buf[type_offset + 1]]);
                        let ev_code =
                            u16::from_ne_bytes([buf[type_offset + 2], buf[type_offset + 3]]);
                        let ev_value = i32::from_ne_bytes([
                            buf[type_offset + 4],
                            buf[type_offset + 5],
                            buf[type_offset + 6],
                            buf[type_offset + 7],
                        ]);

                        // 1. Botón físico de encendido / inicio (cualquier tecla EV_KEY distinta al toque BTN_TOUCH = 330):
                        // Se evalúa siempre que button_armed esté activo
                        if ev_type == 1
                            && ev_value == 1
                            && ev_code != 330
                            && ev_code != 325
                            && btn_armed.load(Ordering::SeqCst)
                        {
                            exit_f.store(true, Ordering::SeqCst);
                            action_f.store(ACTION_EXIT, Ordering::SeqCst);
                            notify_waiters(&notify);
                            break;
                        }

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

                        // EV_KEY = 1, BTN_TOUCH = 330 (toque en pantalla; las coordenadas se evalúan en EV_SYN)
                        if ev_type == 1 && ev_value == 1 && ev_code == 330 {
                            touch_down_in_frame = true;
                        }

                        // EV_SYN = 0, SYN_REPORT = 0 (fin de trama: current_x y current_y ya están actualizados)
                        if ev_type == 0 && ev_code == 0 && touch_down_in_frame {
                            touch_down_in_frame = false;
                            if let Some(act) = classify_touch(
                                current_x,
                                current_y,
                                screen_w,
                                screen_h,
                                any_touch.load(Ordering::SeqCst),
                            ) {
                                match act {
                                    UserAction::Exit => {
                                        exit_f.store(true, Ordering::SeqCst);
                                        action_f.store(ACTION_EXIT, Ordering::SeqCst);
                                        notify_waiters(&notify);
                                        break;
                                    }
                                    UserAction::PagePrev => {
                                        action_f.store(ACTION_PAGE_PREV, Ordering::SeqCst);
                                        notify_waiters(&notify);
                                    }
                                    UserAction::PageNext => {
                                        action_f.store(ACTION_PAGE_NEXT, Ordering::SeqCst);
                                        notify_waiters(&notify);
                                    }
                                }
                            }
                        }
                    } else {
                        consecutive_errors = consecutive_errors.saturating_add(1);
                        if consecutive_errors >= 3 {
                            // Dispositivo no legible o desconectado: terminar el hilo para no ciclar CPU
                            break;
                        }
                        thread::sleep(Duration::from_millis(200));
                    }
                }
            });
        }
    }
}

/// Clasifica la zona de la pantalla donde ocurrió el toque:
/// - Esquina superior derecha: Botón [SALIR]
/// - Debajo de la barra superior:
///   - Mitad izquierda: Página previa / eventos anteriores
///   - Mitad derecha: Página siguiente / eventos siguientes
pub fn classify_touch(
    x: i32,
    y: i32,
    screen_w: u32,
    screen_h: u32,
    any_touch_exits: bool,
) -> Option<UserAction> {
    if any_touch_exits {
        return Some(UserAction::Exit);
    }

    if x < 0 || y < 0 {
        return None;
    }

    // 1. Esquina superior derecha: Botón [SALIR]
    let exit_min_x = (screen_w as f32 * 0.70) as i32;
    let header_h = (screen_h as f32 * 0.17).max(210.0) as i32;

    if x >= exit_min_x && y <= header_h {
        return Some(UserAction::Exit);
    }

    // 2. Debajo de la barra superior (y > header_h):
    if y > header_h {
        let mid_x = (screen_w / 2) as i32;
        if x < mid_x {
            return Some(UserAction::PagePrev);
        } else {
            return Some(UserAction::PageNext);
        }
    }

    None
}

/// Verifica si el toque ocurrió específicamente sobre el botón `[ SALIR ]`
#[allow(dead_code)]
pub fn is_touch_inside_exit_button(
    x: i32,
    y: i32,
    screen_w: u32,
    screen_h: u32,
    any_touch_exits: bool,
) -> bool {
    matches!(
        classify_touch(x, y, screen_w, screen_h, any_touch_exits),
        Some(UserAction::Exit)
    )
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
        // En pantalla de error (any_touch_exits = true), cualquier toque sale
        assert!(is_touch_inside_exit_button(500, 700, w, h, true));
    }

    #[test]
    fn test_classify_touch_zones() {
        let (w, h) = (1072, 1448);
        // Arriba derecha -> Exit
        assert_eq!(classify_touch(900, 65, w, h, false), Some(UserAction::Exit));

        // Arriba izquierda (reloj/fecha) -> None (ignorado)
        assert_eq!(classify_touch(200, 65, w, h, false), None);

        // Mitad inferior izquierda (x < 536, y > 246) -> PagePrev
        assert_eq!(classify_touch(300, 500, w, h, false), Some(UserAction::PagePrev));

        // Mitad inferior derecha (x >= 536, y > 246) -> PageNext
        assert_eq!(classify_touch(800, 500, w, h, false), Some(UserAction::PageNext));
    }
}
