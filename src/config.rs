use serde::Deserialize;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_KINDLE_CONFIG: &str = "/mnt/us/kauwlendar/calendars.toml";
pub const LOCAL_CONFIG: &str = "calendars.toml";

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeFormat {
    #[serde(rename = "12h")]
    H12,
    #[serde(rename = "24h")]
    H24,
}

impl Default for TimeFormat {
    fn default() -> Self {
        TimeFormat::H24
    }
}

impl fmt::Display for TimeFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimeFormat::H12 => write!(f, "12h"),
            TimeFormat::H24 => write!(f, "24h"),
        }
    }
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Es,
    #[default]
    En,
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Language::Es => write!(f, "es"),
            Language::En => write!(f, "en"),
        }
    }
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum DashboardView {
    #[default]
    Timeline,
    Cards,
}

impl fmt::Display for DashboardView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DashboardView::Timeline => write!(f, "timeline"),
            DashboardView::Cards => write!(f, "cards"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NightSchedule {
    pub start_minutes: u32,
    pub end_minutes: u32,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Config {
    #[serde(default = "default_refresh_minutes")]
    pub refresh_minutes: u32,

    #[serde(default)]
    pub utc_offset_hours: i32,

    #[serde(default)]
    pub time_format: TimeFormat,

    #[serde(default)]
    pub language: Language,

    #[serde(default)]
    pub view: DashboardView,

    #[serde(default = "default_night_start")]
    pub night_start: String,

    #[serde(default = "default_night_end")]
    pub night_end: String,

    #[serde(default)]
    pub calendars: Vec<CalendarSource>,
}

fn default_refresh_minutes() -> u32 {
    60
}

fn default_night_start() -> String {
    "21:00".to_string()
}

fn default_night_end() -> String {
    "06:00".to_string()
}

fn parse_hhmm_or_off(
    field: &'static str,
    raw: &str,
) -> Result<Option<(u32, u32)>, ConfigError> {
    let trimmed = raw.trim();
    if trimmed.eq_ignore_ascii_case("off") {
        return Ok(None);
    }

    let mut parts = trimmed.split(':');
    let (Some(h_str), Some(m_str), None) = (parts.next(), parts.next(), parts.next()) else {
        return Err(ConfigError::InvalidNightTime {
            field,
            value: raw.to_string(),
        });
    };

    let (Ok(hour), Ok(minute)) = (h_str.parse::<u32>(), m_str.parse::<u32>()) else {
        return Err(ConfigError::InvalidNightTime {
            field,
            value: raw.to_string(),
        });
    };

    if hour > 23 || minute > 59 {
        return Err(ConfigError::InvalidNightTime {
            field,
            value: raw.to_string(),
        });
    }

    Ok(Some((hour, minute)))
}

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct CalendarSource {
    pub name: String,
    pub url: String,
}

#[derive(Debug)]
pub enum ConfigError {
    FileNotFound(String),
    ReadError(String),
    ParseError(String),
    NoCalendarsConfigured,
    InvalidUrl { calendar_name: String, url: String },
    InvalidNightTime { field: &'static str, value: String },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::FileNotFound(path) => {
                write!(
                    f,
                    "No se encontró el archivo de configuración en:\n{}",
                    path
                )
            }
            ConfigError::ReadError(err) => write!(f, "Error al leer configuración: {}", err),
            ConfigError::ParseError(err) => write!(f, "Error de sintaxis en TOML: {}", err),
            ConfigError::NoCalendarsConfigured => write!(
                f,
                "No hay calendarios configurados.\nAgrega al menos un [[calendars]] con su nombre y url."
            ),
            ConfigError::InvalidUrl { calendar_name, url } => write!(
                f,
                "URL inválida para el calendario '{}':\n{}\nDebe comenzar con http:// o https://",
                calendar_name, url
            ),
            ConfigError::InvalidNightTime { field, value } => write!(
                f,
                "Horario nocturno inválido en '{}': '{}'\nUsa formato \"HH:MM\" (ej. \"21:00\") o \"off\" para actualizar 24h.",
                field, value
            ),
        }
    }
}

impl std::error::Error for ConfigError {}

impl Config {
    pub fn find_config_path() -> PathBuf {
        if let Ok(env_path) = std::env::var("KAUWLENDAR_CONFIG") {
            let p = PathBuf::from(env_path);
            if p.exists() {
                return p;
            }
        }

        let local = Path::new(LOCAL_CONFIG);
        if local.exists() {
            return local.to_path_buf();
        }

        let kindle = Path::new(DEFAULT_KINDLE_CONFIG);
        if kindle.exists() {
            return kindle.to_path_buf();
        }

        // Si ninguno existe, retornamos local si estamos en dev o Kindle si estamos en Kindle
        if Path::new("/mnt/us").exists() {
            kindle.to_path_buf()
        } else {
            local.to_path_buf()
        }
    }

    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Config, ConfigError> {
        let path_ref = path.as_ref();
        if !path_ref.exists() {
            return Err(ConfigError::FileNotFound(path_ref.display().to_string()));
        }

        let content = fs::read_to_string(path_ref)
            .map_err(|e| ConfigError::ReadError(format!("{}: {}", path_ref.display(), e)))?;

        let config: Config = toml::from_str(&content)
            .map_err(|e| ConfigError::ParseError(e.to_string()))?;

        config.validate()?;
        Ok(config)
    }

    pub fn load() -> Result<(Config, PathBuf), ConfigError> {
        let path = Self::find_config_path();
        let cfg = Self::load_from_file(&path)?;
        Ok((cfg, path))
    }

    pub fn night_schedule(&self) -> Result<Option<NightSchedule>, ConfigError> {
        let start = parse_hhmm_or_off("night_start", &self.night_start)?;
        let end = parse_hhmm_or_off("night_end", &self.night_end)?;

        match (start, end) {
            (Some((sh, sm)), Some((eh, em))) => {
                let start_minutes = sh * 60 + sm;
                let end_minutes = eh * 60 + em;
                if start_minutes == end_minutes {
                    Ok(None)
                } else {
                    Ok(Some(NightSchedule {
                        start_minutes,
                        end_minutes,
                    }))
                }
            }
            _ => Ok(None),
        }
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.calendars.is_empty() {
            return Err(ConfigError::NoCalendarsConfigured);
        }

        for cal in &self.calendars {
            let trimmed = cal.url.trim();
            if !trimmed.starts_with("http://")
                && !trimmed.starts_with("https://")
                && !trimmed.starts_with("file://")
            {
                return Err(ConfigError::InvalidUrl {
                    calendar_name: cal.name.clone(),
                    url: cal.url.clone(),
                });
            }
        }

        self.night_schedule()?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_config() {
        let toml_data = r#"
            refresh_minutes = 30
            utc_offset_hours = -4
            time_format = "12h"

            [[calendars]]
            name = "Personal"
            url = "https://calendar.google.com/test.ics"
        "#;

        let cfg: Config = toml::from_str(toml_data).expect("Valid config");
        assert_eq!(cfg.refresh_minutes, 30);
        assert_eq!(cfg.utc_offset_hours, -4);
        assert_eq!(cfg.time_format, TimeFormat::H12);
        assert_eq!(cfg.language, Language::En); // Default is En
        assert_eq!(cfg.view, DashboardView::Timeline); // Default is Timeline
        assert_eq!(cfg.night_start, "21:00");
        assert_eq!(cfg.night_end, "06:00");
        assert_eq!(
            cfg.night_schedule().expect("Valid night schedule"),
            Some(NightSchedule {
                start_minutes: 21 * 60,
                end_minutes: 6 * 60,
            })
        );
        assert_eq!(cfg.calendars.len(), 1);
        assert_eq!(cfg.calendars[0].name, "Personal");
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_parse_language_english() {
        let toml_data = r#"
            language = "en"
            view = "cards"
            [[calendars]]
            name = "Work"
            url = "https://outlook.com/test.ics"
        "#;
        let cfg: Config = toml::from_str(toml_data).expect("Valid config");
        assert_eq!(cfg.language, Language::En);
        assert_eq!(cfg.view, DashboardView::Cards);
    }

    #[test]
    fn test_missing_calendars_fails_validation() {
        let toml_data = r#"
            refresh_minutes = 30
            utc_offset_hours = -4
        "#;

        let cfg: Config = toml::from_str(toml_data).expect("Valid toml");
        assert!(matches!(
            cfg.validate(),
            Err(ConfigError::NoCalendarsConfigured)
        ));
    }

    #[test]
    fn test_invalid_url_fails_validation() {
        let toml_data = r#"
            [[calendars]]
            name = "Test"
            url = "ftp://invalid-url.com"
        "#;

        let cfg: Config = toml::from_str(toml_data).expect("Valid toml");
        assert!(matches!(
            cfg.validate(),
            Err(ConfigError::InvalidUrl { .. })
        ));
    }

    #[test]
    fn test_night_schedule_custom_and_off() {
        let custom_toml = r#"
            night_start = "22:30"
            night_end = "06:45"
            [[calendars]]
            name = "Test"
            url = "https://example.com/cal.ics"
        "#;
        let cfg: Config = toml::from_str(custom_toml).expect("Valid toml");
        assert!(cfg.validate().is_ok());
        assert_eq!(
            cfg.night_schedule().unwrap(),
            Some(NightSchedule {
                start_minutes: 22 * 60 + 30,
                end_minutes: 6 * 60 + 45,
            })
        );

        let off_toml = r#"
            night_start = "off"
            [[calendars]]
            name = "Test"
            url = "https://example.com/cal.ics"
        "#;
        let cfg_off: Config = toml::from_str(off_toml).expect("Valid toml");
        assert!(cfg_off.validate().is_ok());
        assert_eq!(cfg_off.night_schedule().unwrap(), None);
    }

    #[test]
    fn test_invalid_night_schedule_fails_validation() {
        let bad_toml = r#"
            night_start = "25:00"
            [[calendars]]
            name = "Test"
            url = "https://example.com/cal.ics"
        "#;
        let cfg: Config = toml::from_str(bad_toml).expect("Valid toml");
        assert!(matches!(
            cfg.validate(),
            Err(ConfigError::InvalidNightTime {
                field: "night_start",
                ..
            })
        ));
    }
}
