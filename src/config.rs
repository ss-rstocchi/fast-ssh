use crate::Theme;
use anyhow::{format_err, Context, Result};
use serde::Deserialize;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub theme: Theme,
}

pub fn resolve_config() -> Config {
    let loaded = config_file_path().and_then(|path| {
        // A missing default file only costs discoverability, not the theme
        if let Err(e) = write_default_config(&path) {
            eprintln!("Warning: {:#}", e);
        }
        load_config(&path)
    });

    match loaded {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Warning: Failed to load user config: {:#}", e);
            eprintln!("Using default theme configuration");
            Config {
                theme: Theme::default(),
            }
        }
    }
}

fn config_file_path() -> Result<PathBuf> {
    let config_dir =
        dirs::config_dir().ok_or_else(|| format_err!("Could not get config directory"))?;
    Ok(config_dir.join("FastSSH").join("config.yaml"))
}

/// Creates `path` with the default config so users have a file to edit.
/// An existing file is never touched.
fn write_default_config(path: &Path) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).with_context(|| format!("Could not create {}", dir.display()))?;
    }

    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut file) => file
            .write_all(DEFAULT_CONFIG.as_bytes())
            .with_context(|| format!("Could not write {}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(e).with_context(|| format!("Could not create {}", path.display())),
    }
}

fn load_config(path: &Path) -> Result<Config> {
    let text =
        fs::read_to_string(path).with_context(|| format!("Could not read {}", path.display()))?;
    parse_config(&text)
}

fn parse_config(text: &str) -> Result<Config> {
    match serde_yaml::from_str::<Option<Config>>(text) {
        Ok(Some(conf)) => Ok(conf),
        Ok(None) => Err(format_err!("Config file is empty or invalid")),
        Err(e) => Err(format_err!(
            "Error parsing config file, make sure format is valid: {}",
            e
        )),
    }
}

const DEFAULT_CONFIG: &str = "
# This is the default configuration for FastSSH.

theme:
    text_primary: \"#b967ff\"
    text_secondary: \"#ffffff\"
    border_color: \"#b967ff\"
";

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_config_path(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("fast_ssh_config_{}_{}", std::process::id(), name))
            .join("config.yaml")
    }

    #[test]
    fn test_write_default_config_creates_but_never_overwrites() {
        let path = temp_config_path("write");
        let _ = fs::remove_file(&path);

        write_default_config(&path).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), DEFAULT_CONFIG);
        assert!(load_config(&path).is_ok());

        fs::write(&path, "theme:\n    text_primary: \"#ff0000\"").unwrap();
        write_default_config(&path).unwrap();
        let config = load_config(&path).unwrap();
        assert_eq!(
            config.theme.text_primary(),
            tui::style::Color::Rgb(255, 0, 0)
        );

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn test_load_config_errors() {
        assert!(load_config(&temp_config_path("missing")).is_err());
        assert!(parse_config("null").is_err());
        assert!(parse_config("this is not valid yaml: [[[").is_err());
    }

    #[test]
    fn test_default_config_is_valid_yaml() {
        // Test that the default config string parses correctly
        let result = serde_yaml::from_str::<Option<Config>>(DEFAULT_CONFIG);
        assert!(result.is_ok());

        if let Ok(Some(config)) = result {
            // Verify default theme values can be accessed
            let _ = config.theme.text_primary();
            let _ = config.theme.text_secondary();
            let _ = config.theme.border_color();
        }
    }

    #[test]
    fn test_config_with_custom_theme() {
        let yaml = "theme:\n    text_primary: \"#ff0000\"\n    text_secondary: \"#00ff00\"\n    border_color: \"#0000ff\"";
        let config: Option<Config> = serde_yaml::from_str(yaml).expect("Failed to parse config");
        assert!(config.is_some());

        let config = config.unwrap();
        assert_eq!(
            config.theme.text_primary(),
            tui::style::Color::Rgb(255, 0, 0)
        );
        assert_eq!(
            config.theme.text_secondary(),
            tui::style::Color::Rgb(0, 255, 0)
        );
        assert_eq!(
            config.theme.border_color(),
            tui::style::Color::Rgb(0, 0, 255)
        );
    }

    #[test]
    fn test_config_with_partial_theme() {
        let yaml = "theme:\n    text_primary: \"#ff0000\"";
        let config: Option<Config> = serde_yaml::from_str(yaml).expect("Failed to parse config");
        assert!(config.is_some());

        let config = config.unwrap();
        assert_eq!(
            config.theme.text_primary(),
            tui::style::Color::Rgb(255, 0, 0)
        );
        assert_eq!(config.theme.text_secondary(), tui::style::Color::White);
        assert_eq!(
            config.theme.border_color(),
            tui::style::Color::Rgb(185, 103, 255)
        );
    }
}
