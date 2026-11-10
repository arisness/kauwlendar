use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

const FBINK_CANDIDATES: &[&str] = &[
    "/mnt/us/koreader/fbink",
    "/mnt/us/usbnet/bin/fbink",
    "/var/local/kmc/bin/fbink",
    "/mnt/us/kpm/bin/fbink",
    "/usr/bin/fbink",
    "/usr/sbin/fbink",
    "fbink",
];

pub fn find_fbink_bin() -> &'static str {
    for candidate in FBINK_CANDIDATES {
        if *candidate != "fbink" && Path::new(candidate).exists() {
            return candidate;
        }
    }
    "fbink"
}

fn run_quiet(cmd: &str, args: &[&str]) -> bool {
    Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub struct KindleGuard {
    active: bool,
    pub initial_wifi: bool,
}

impl KindleGuard {
    pub fn new() -> Self {
        let is_k = is_kindle();
        let initial_wifi = if is_k {
            get_wifi_state()
        } else {
            true
        };
        if is_k {
            // Evitar que powerd active el salvapantallas sin matar el servicio powerd
            run_quiet("lipc-set-prop", &["com.lab126.powerd", "preventScreenSaver", "1"]);
        }
        Self {
            active: is_k,
            initial_wifi,
        }
    }

    /// Pausa los procesos de interfaz gráfica de Kindle justo antes de dibujar en el framebuffer
    /// para que ni KPM ni el visor de scriptlets sobreescriban la pantalla.
    pub fn pause_ui(&self) {
        if self.active {
            for proc_name in &["cvm", "awesome", "mesquite"] {
                run_quiet("killall", &["-STOP", proc_name]);
            }
        }
    }

    pub fn restore(&mut self) {
        if self.active {
            // 1. Restaurar el estado original del Wi-Fi del Kindle
            set_wifi(self.initial_wifi);

            // 2. Reanudar inmediatamente los procesos de la interfaz gráfica de Kindle
            for proc_name in &["cvm", "awesome", "mesquite"] {
                run_quiet("killall", &["-CONT", proc_name]);
            }

            // 3. Restaurar el comportamiento normal del salvapantallas en powerd
            run_quiet("lipc-set-prop", &["com.lab126.powerd", "preventScreenSaver", "0"]);

            // 4. Limpiar el framebuffer con refresco completo para que la UI de Kindle se repinte limpia
            let fbink = find_fbink_bin();
            if !run_quiet(fbink, &["-q", "-c", "-f"]) {
                for eips in &["/usr/sbin/eips", "/usr/bin/eips", "eips"] {
                    if run_quiet(eips, &["-c"]) {
                        break;
                    }
                }
            }

            self.active = false;
        }
    }
}

impl Drop for KindleGuard {
    fn drop(&mut self) {
        self.restore();
    }
}

pub fn is_kindle() -> bool {
    // Si existe la partición típica de Kindle
    Path::new("/mnt/us").exists()
}

/// Normaliza dimensiones de framebuffer de Kindle corrigiendo buffers dobles/triples
/// (ej. 1072x2896 -> 1072x1448 o 1448x2144 -> 1072x1448) y orientación horizontal (1448x1072 -> 1072x1448).
pub fn normalize_kindle_dimensions(mut w: u32, mut h: u32) -> (u32, u32) {
    if w == 0 || h == 0 {
        return (1072, 1448);
    }

    let is_kindle_aspect = |a: u32, b: u32| -> bool {
        if a == 0 || b == 0 {
            return false;
        }
        let ratio = (a.max(b) as f64) / (a.min(b) as f64);
        (1.28..=1.40).contains(&ratio)
    };

    // Si aún no tiene la proporción 4:3 típica de Kindle (~1.33 - 1.35),
    // comprobar si el alto o ancho virtual está multiplicado x2 o x3 por double/triple buffering
    if !is_kindle_aspect(w, h) {
        for divisor in [2u32, 3, 4] {
            if h % divisor == 0 && is_kindle_aspect(w, h / divisor) {
                h /= divisor;
                break;
            }
            if w % divisor == 0 && is_kindle_aspect(w / divisor, h) {
                w /= divisor;
                break;
            }
        }
    }

    // Asegurar orientación vertical (portrait: ancho < alto) para el dashboard
    if w > h {
        std::mem::swap(&mut w, &mut h);
    }

    (w, h)
}

/// Extrae `viewWidth` y `viewHeight` (o `screenWidth` y `screenHeight`) de la salida de `fbink -e`.
pub fn parse_fbink_eval_dimensions(output: &str) -> Option<(u32, u32)> {
    let mut view_w: Option<u32> = None;
    let mut view_h: Option<u32> = None;
    let mut screen_w: Option<u32> = None;
    let mut screen_h: Option<u32> = None;

    for token in output.split(&[';', '\n', '\r', ' '][..]) {
        let token = token.trim();
        if let Some((k, v)) = token.split_once('=') {
            let v_clean = v.trim_matches(&['"', '\''][..]);
            if let Ok(val) = v_clean.parse::<u32>() {
                match k.trim() {
                    "viewWidth" => view_w = Some(val),
                    "viewHeight" => view_h = Some(val),
                    "screenWidth" => screen_w = Some(val),
                    "screenHeight" => screen_h = Some(val),
                    _ => {}
                }
            }
        }
    }

    if let (Some(w), Some(h)) = (view_w, view_h) {
        if w > 0 && h > 0 {
            return Some(normalize_kindle_dimensions(w, h));
        }
    }
    if let (Some(w), Some(h)) = (screen_w, screen_h) {
        if w > 0 && h > 0 {
            return Some(normalize_kindle_dimensions(w, h));
        }
    }
    None
}

pub fn get_screen_dimensions() -> (u32, u32) {
    if is_kindle() {
        // 1. Consultar directamente a `fbink -e` (reporta el tamaño real visible de la pantalla)
        let fbink = find_fbink_bin();
        if let Ok(out) = Command::new(fbink).arg("-e").output() {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                if let Some((w, h)) = parse_fbink_eval_dimensions(&stdout) {
                    return (w, h);
                }
            }
        }

        // 2. Fallback: leer virtual_size y normalizar double-buffering / orientación
        if let Ok(content) = fs::read_to_string("/sys/class/graphics/fb0/virtual_size") {
            let parts: Vec<&str> = content.trim().split(&[',', ' '][..]).collect();
            if parts.len() >= 2 {
                if let (Ok(w), Ok(h)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                    if w > 0 && h > 0 {
                        return normalize_kindle_dimensions(w, h);
                    }
                }
            }
        }
    }
    // Resolución por defecto de Kindle Paperwhite
    (1072, 1448)
}

pub fn get_wifi_state() -> bool {
    if is_kindle() {
        if let Ok(out) = Command::new("lipc-get-prop")
            .args(["com.lab126.cmd", "wirelessEnable"])
            .output()
        {
            let text = String::from_utf8_lossy(&out.stdout);
            return text.trim() == "1";
        }
    }
    true
}

pub fn set_wifi(enable: bool) {
    if is_kindle() {
        let val = if enable { "1" } else { "0" };

        // Intentar con flag entera (-i) y sin ella para compatibilidad con todas las versiones de Kindle
        let _ = Command::new("lipc-set-prop")
            .args(["-i", "com.lab126.cmd", "wirelessEnable", val])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        let _ = Command::new("lipc-set-prop")
            .args(["com.lab126.cmd", "wirelessEnable", val])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        let _ = Command::new("lipc-set-prop")
            .args(["-i", "com.lab126.wifid", "enable", val])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        let _ = Command::new("lipc-set-prop")
            .args(["com.lab126.wifid", "enable", val])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

pub fn wait_for_network(timeout_secs: u32) -> bool {
    if !is_kindle() {
        return true;
    }

    for _ in 0..(timeout_secs * 2) {
        if let Ok(out) = Command::new("ip").args(["-4", "addr", "show", "wlan0"]).output() {
            let stdout = String::from_utf8_lossy(&out.stdout);
            if stdout.contains("inet ") {
                return true;
            }
        }
        thread::sleep(Duration::from_millis(500));
    }
    false
}

pub fn get_battery_percent() -> Option<u32> {
    if is_kindle() {
        // 1. Intentar leer por lipc (funciona si powerd está activo)
        if let Ok(out) = Command::new("lipc-get-prop").args(["com.lab126.powerd", "battLevel"]).output() {
            let text = String::from_utf8_lossy(&out.stdout);
            if let Ok(lvl) = text.trim().parse::<u32>() {
                if lvl > 0 && lvl <= 100 {
                    return Some(lvl);
                }
            }
        }

        // 2. Escanear dinámicamente cualquier carpeta en /sys/class/power_supply/*/capacity
        if let Ok(entries) = fs::read_dir("/sys/class/power_supply") {
            for entry in entries.flatten() {
                let capacity_file = entry.path().join("capacity");
                if capacity_file.exists() {
                    if let Ok(content) = fs::read_to_string(&capacity_file) {
                        if let Ok(lvl) = content.trim().parse::<u32>() {
                            if lvl > 0 && lvl <= 100 {
                                return Some(lvl);
                            }
                        }
                    }
                }
            }
        }

        // 3. Modelos específicos de Kindle (yoshi_battery)
        let yoshi_path = "/sys/devices/system/yoshi_battery/yoshi_battery0/battery_capacity";
        if let Ok(content) = fs::read_to_string(yoshi_path) {
            if let Ok(lvl) = content.trim().parse::<u32>() {
                if lvl > 0 && lvl <= 100 {
                    return Some(lvl);
                }
            }
        }

        // 4. Intentar calcular a partir de charge_now y charge_full si capacity no existe
        if let Ok(entries) = fs::read_dir("/sys/class/power_supply") {
            for entry in entries.flatten() {
                let now_file = entry.path().join("charge_now");
                let full_file = entry.path().join("charge_full");
                if now_file.exists() && full_file.exists() {
                    if let (Ok(now_str), Ok(full_str)) = (fs::read_to_string(now_file), fs::read_to_string(full_file)) {
                        if let (Ok(now), Ok(full)) = (now_str.trim().parse::<u64>(), full_str.trim().parse::<u64>()) {
                            if full > 0 {
                                let pct = ((now as f64 / full as f64) * 100.0).round() as u32;
                                return Some(pct.min(100));
                            }
                        }
                    }
                }
            }
        }
    } else {
        // En Linux Desktop intentar leer batería si es laptop
        if let Ok(content) = fs::read_to_string("/sys/class/power_supply/BAT0/capacity") {
            if let Ok(lvl) = content.trim().parse::<u32>() {
                return Some(lvl.min(100));
            }
        }
    }

    Some(100)
}

#[allow(dead_code)]
pub fn sleep_with_rtcwake(seconds: i64) {
    if !is_kindle() {
        println!("[DEV] Esperando {} segundos (modo PC)...", seconds);
        thread::sleep(Duration::from_secs(seconds.max(1) as u64));
        return;
    }

    let _ = fs::write("/sys/class/rtc/rtc0/device/power/wakeup", "enabled");
    let _ = fs::write("/sys/class/rtc/rtc1/device/power/wakeup", "enabled");

    let sec_str = seconds.to_string();

    let status_rtc0 = Command::new("rtcwake")
        .args(["-d", "rtc0", "-m", "mem", "-s", &sec_str])
        .status();

    if status_rtc0.map(|s| !s.success()).unwrap_or(true) {
        let status_rtc1 = Command::new("rtcwake")
            .args(["-d", "rtc1", "-m", "mem", "-s", &sec_str])
            .status();

        if status_rtc1.map(|s| !s.success()).unwrap_or(true) {
            thread::sleep(Duration::from_secs(seconds as u64));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_kindle_dimensions_double_buffer_and_landscape() {
        // Double-buffered portrait (Paperwhite 1072x2896 -> 1072x1448)
        assert_eq!(normalize_kindle_dimensions(1072, 2896), (1072, 1448));
        // Double-buffered portrait (PW5 1236x3296 -> 1236x1648)
        assert_eq!(normalize_kindle_dimensions(1236, 3296), (1236, 1648));
        // Landscape native panel scanout (1448x1072 -> 1072x1448)
        assert_eq!(normalize_kindle_dimensions(1448, 1072), (1072, 1448));
        // Double-buffered landscape (1448x2144 -> 1072x1448)
        assert_eq!(normalize_kindle_dimensions(1448, 2144), (1072, 1448));
        // Standard portrait untouched
        assert_eq!(normalize_kindle_dimensions(1072, 1448), (1072, 1448));
    }

    #[test]
    fn test_parse_fbink_eval_dimensions() {
        let sample = "isKindle=1;deviceName=\"Paperwhite\";viewWidth=1236;viewHeight=1648;screenWidth=1236;screenHeight=1648;";
        assert_eq!(parse_fbink_eval_dimensions(sample), Some((1236, 1648)));
    }
}
