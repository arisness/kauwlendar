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
    pub calendars: Vec<CalendarSource>,
}

fn default_refresh_minutes() -> u32 {
    60
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
        assert_eq!(cfg.calendars.len(), 1);
        assert_eq!(cfg.calendars[0].name, "Personal");
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_parse_language_english() {
        let toml_data = r#"
            language = "en"
            [[calendars]]
            name = "Work"
            url = "https://outlook.com/test.ics"
        "#;
        let cfg: Config = toml::from_str(toml_data).expect("Valid config");
        assert_eq!(cfg.language, Language::En);
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
}
